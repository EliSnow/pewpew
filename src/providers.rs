mod csv_reader;
mod json_reader;
mod line_reader;

use self::{csv_reader::CsvReader, json_reader::JsonReader, line_reader::LineReader};

use crate::channel::{self, Limit};
use crate::config;
use crate::error::TestError;
use crate::load_test::TestEndReason;
use crate::util::{json_value_into_string, tweak_path, Either, Either3};

use futures::{stream, sync::mpsc::Sender as FCSender, Future, Stream};
use serde_json as json;
use tokio::{fs::File as TokioFile, prelude::*};
use tokio_threadpool::blocking;

use std::{io, path::PathBuf, sync::Arc};

pub struct Provider {
    pub auto_return: Option<config::EndpointProvidesSendOptions>,
    pub rx: channel::Receiver<json::Value>,
    pub tx: channel::Sender<json::Value>,
    pub on_demand: channel::OnDemandReceiver<json::Value>,
}

impl Provider {
    fn new(
        auto_return: Option<config::EndpointProvidesSendOptions>,
        rx: channel::Receiver<json::Value>,
        tx: channel::Sender<json::Value>,
    ) -> Self {
        Provider {
            auto_return,
            tx,
            rx: rx.clone(),
            on_demand: channel::OnDemandReceiver::new(rx),
        }
    }
}

pub fn file(
    mut template: config::FileProvider,
    test_killer: FCSender<Result<TestEndReason, TestError>>,
    config_path: &PathBuf,
) -> Result<Provider, TestError> {
    tweak_path(&mut template.path, config_path);
    let file = template.path.clone();
    let test_killer2 = test_killer.clone();
    let stream = match template.format {
        config::FileFormat::Csv => Either3::A(
            CsvReader::new(&template)
                .map_err(|e| {
                    TestError::Other(
                        format!("creating file reader from file `{}`: {}", file, e).into(),
                    )
                })?
                .into_stream(),
        ),
        config::FileFormat::Json => Either3::B(
            JsonReader::new(&template)
                .map_err(|e| {
                    TestError::Other(
                        format!("creating file reader from file `{}`: {}", file, e).into(),
                    )
                })?
                .into_stream(),
        ),
        config::FileFormat::Line => Either3::C(
            LineReader::new(&template)
                .map_err(|e| {
                    TestError::Other(
                        format!("creating file reader from file `{}`: {}", file, e).into(),
                    )
                })?
                .into_stream(),
        ),
    };
    let (tx, rx) = channel::channel(template.buffer);
    let tx2 = tx.clone();
    let prime_tx = stream
        .map_err(move |e| {
            let e = TestError::Other(format!("reading file `{}`: {}", file, e).into());
            channel::ChannelClosed::wrapped(e)
        })
        .forward(tx2)
        .map(|_| ())
        .or_else(move |e| match e.inner_cast() {
            Some(e) => Either::A(test_killer2.send(Err(*e)).then(|_| Ok(()))),
            None => Either::B(Ok(()).into_future()),
        });

    tokio::spawn(prime_tx);
    Ok(Provider::new(template.auto_return, rx, tx))
}

#[must_use = "streams do nothing unless polled"]
struct RepeaterStream<T> {
    i: usize,
    values: Vec<T>,
}

impl<T> RepeaterStream<T> {
    fn new(values: Vec<T>) -> Self {
        RepeaterStream { i: 0, values }
    }
}

impl<T: Clone> Stream for RepeaterStream<T> {
    type Item = T;
    type Error = channel::ChannelClosed;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        let i = self.i;
        self.i = (self.i + 1).checked_rem(self.values.len()).unwrap_or(0);
        Ok(Async::Ready(self.values.get(i).cloned()))
    }
}

pub fn response(template: config::ResponseProvider) -> Provider {
    let (tx, rx) = channel::channel(template.buffer);
    Provider::new(template.auto_return, rx, tx)
}

pub fn literals(
    values: Vec<json::Value>,
    auto_return: Option<config::EndpointProvidesSendOptions>,
) -> Provider {
    let rs: RepeaterStream<json::Value> = RepeaterStream::new(values);
    let (tx, rx) = channel::channel(Limit::auto());
    let tx2 = tx.clone();
    let prime_tx = rs
        .forward(tx2)
        // Error propagate here when sender channel closes at test conclusion
        .then(|_| Ok(()));
    tokio::spawn(prime_tx);
    Provider::new(auto_return, rx, tx)
}

pub fn range(range: config::RangeProvider) -> Provider {
    let (tx, rx) = channel::channel(Limit::auto());
    let prime_tx = stream::iter_ok::<_, channel::ChannelClosed>(range.0.map(json::Value::from))
        // .map_err(|_| channel::ChannelClosed)
        .forward(tx.clone())
        // Error propagate here when sender channel closes at test conclusion
        .then(|_| Ok(()));
    tokio::spawn(prime_tx);
    Provider::new(None, rx, tx)
}

pub fn logger(
    mut template: config::Logger,
    test_killer: FCSender<Result<TestEndReason, TestError>>,
    config_path: &PathBuf,
) -> channel::Sender<json::Value> {
    let (tx, rx) = channel::channel::<json::Value>(Limit::Integer(5));
    let limit = template.limit;
    let pretty = template.pretty;
    let kill = template.kill;
    let mut counter = 0;
    let mut keep_logging = true;
    match template.to.as_str() {
        "stderr" => {
            let logger = rx
                .for_each(move |v| {
                    counter += 1;
                    if keep_logging {
                        if pretty && !v.is_string() {
                            eprintln!("{:#}", v);
                        } else {
                            eprintln!("{}", json_value_into_string(v));
                        }
                    }
                    match limit {
                        Some(limit) if counter >= limit => {
                            if kill {
                                Either3::B(
                                    test_killer
                                        .clone()
                                        .send(Err(TestError::KilledByLogger))
                                        .then(|_| Ok(())),
                                )
                            } else {
                                keep_logging = false;
                                Either3::A(Ok(()).into_future())
                            }
                        }
                        None if kill => Either3::C(
                            test_killer
                                .clone()
                                .send(Err(TestError::KilledByLogger))
                                .then(|_| Ok(())),
                        ),
                        _ => Either3::A(Ok(()).into_future()),
                    }
                })
                .then(|_| Ok(()));
            tokio::spawn(logger);
        }
        "stdout" => {
            let logger = rx
                .for_each(move |v| {
                    counter += 1;
                    if keep_logging {
                        if pretty && !v.is_string() {
                            println!("{:#}", v);
                        } else {
                            println!("{}", json_value_into_string(v));
                        }
                    }
                    match limit {
                        Some(limit) if counter >= limit => {
                            if kill {
                                Either3::B(
                                    test_killer
                                        .clone()
                                        .send(Err(TestError::KilledByLogger))
                                        .then(|_| Ok(())),
                                )
                            } else {
                                keep_logging = false;
                                Either3::A(Ok(()).into_future())
                            }
                        }
                        None if kill => Either3::C(
                            test_killer
                                .clone()
                                .send(Err(TestError::KilledByLogger))
                                .then(|_| Ok(())),
                        ),
                        _ => Either3::A(Ok(()).into_future()),
                    }
                })
                .then(|_| Ok(()));
            tokio::spawn(logger);
        }
        _ => {
            tweak_path(&mut template.to, config_path);
            let file_name = Arc::new(template.to);
            let file_name2 = file_name.clone();
            let test_killer2 = test_killer.clone();
            let logger = TokioFile::create((&*file_name).clone())
                .map_err(move |e| {
                    TestError::Other(
                        format!("creating logger file `{:?}`: {}", file_name2, e).into(),
                    )
                })
                .and_then(move |mut file| {
                    rx.map_err(|_| {
                        TestError::Internal("logger receiver unexpectedly errored".into())
                    })
                    .for_each(move |v| {
                        let file_name = file_name.clone();
                        counter += 1;
                        let result = if keep_logging {
                            if pretty {
                                writeln!(file, "{:#}", v)
                            } else {
                                writeln!(file, "{}", v)
                            }
                        } else {
                            Ok(())
                        };
                        let result = result.into_future().map_err(move |e| {
                            TestError::Other(
                                format!("writing to file `{}`: {}", file_name, e).into(),
                            )
                        });
                        match limit {
                            Some(limit) if counter >= limit => {
                                if kill {
                                    Either3::B(
                                        test_killer
                                            .clone()
                                            .send(Err(TestError::KilledByLogger))
                                            .then(|_| Ok(())),
                                    )
                                } else {
                                    keep_logging = false;
                                    Either3::A(result)
                                }
                            }
                            None if kill => Either3::C(
                                test_killer
                                    .clone()
                                    .send(Err(TestError::KilledByLogger))
                                    .then(|_| Ok(())),
                            ),
                            _ => Either3::A(result),
                        }
                    })
                })
                .or_else(move |e| test_killer2.send(Err(e)).then(|_| Ok::<_, ()>(())))
                .then(|_| Ok(()));
            tokio::spawn(logger);
        }
    }
    tx
}

fn into_stream<I: Iterator<Item = Result<json::Value, io::Error>>>(
    mut iter: I,
) -> impl Stream<Item = json::Value, Error = io::Error> {
    stream::poll_fn(move || blocking(|| iter.next()))
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        .and_then(|r| r)
}
