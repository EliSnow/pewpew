# TODOs
- Allow an url's domain to be templated with a static provider
- Update endpoint bodys to be an enum of string or json. Json string values are an enum of a string or reference. Strings can be templates
      make sure the stringify helper is publicly accessible.
- Get rid of all panics, unreachable, etc. Instead there should be a means of killing the test in those cases. This is a prerequisite to having
      a server which runs load tests on demand
- Improvie documentation
- Create a Visual Studio Code language extension for the loadtest file schema
- Make files referenced by a file provider relative to the configuraton file
- Add cli option for results
- every minute we print stats to console, also write those results to disk, overwriting the previous file every minute
- add config options: for client: request timeout, standard headers, keepalive time, dns resolution thread pool
- add `files` body provider
- update `mod_interval` code so that multiple `scale_fn`s can be added so that it handles the transition from
      one fn to the next, rather than using `Stream::chain`. This is important because, currently, if a
      provider value is delayed for a long period of time, it will start on the next `mod_interval` even
      though enough time may have passed that it should skip several `mod_interval`s. Should also help in allowing
      `load_patterns` to be dynamically changed during test run
- url encode url templates (maybe just a helper)
- add more tests - unit and integration - get code coverage
- resolve `static` providers before creating endpoint stream
- add more configuration to file provider. random order; format: string, json, csv
- create template helpers to extract fields from csv
- add in ability to log connection timeouts, connection errors, etc
- track system health (sysinfo crate) perhaps event loop latency and determine if system is overloaded
- ensure RTTs are accurate. Is there any queueing of requests if waiting for an available socket???
- add in machine clustering. Machines should open up a secure connection using a PSK
- create custom executor with priorities. system monitoring and logging should be high priority
      generating load should be low priority
- create stats viewer (raw html + js + css - no server)
- allow load_patterns/config to change while a test is running. Monitor the load test config file for changes
- add test monitoring. Possibly use tui for terminal output. Have webserver which will display a dashboard