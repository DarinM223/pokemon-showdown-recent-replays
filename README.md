Pokemon Showdown Recent Replay Service
======================================

A web service written in Rust using Tokio and Hyper that returns
the recent replays from Pokemon Showdown, mainly written
to show how to write an asynchronous server that uses an asynchronous client
to retrieve pages while moving computationally intensive work into separate threads.

It handles requests to the root path by sending a request to the
Pokemon Showdown replay page and returning the scraped recent replays in JSON format.

To build and run the server run `cargo run` in the project directory. Then you can see the recent replays by visiting `http://localhost:1337/` in the browser.
