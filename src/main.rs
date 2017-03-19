#[macro_use]
extern crate serde_derive;

extern crate futures;
extern crate futures_cpupool;
extern crate hyper;
extern crate kuchiki;
extern crate serde_json;
extern crate tokio_core;

use futures::future;
use futures::Future;
use futures::Stream;
use futures_cpupool::CpuPool;
use hyper::{Error, Get, StatusCode, Url};
use hyper::client::{Client, HttpConnector};
use hyper::header::ContentLength;
use hyper::server::{Http, Request, Response, Service};
use kuchiki::traits::*;
use std::rc::Rc;
use std::str;
use tokio_core::net::TcpListener;
use tokio_core::reactor::{Core, Handle};

const REPLAY_URL: &'static str = "http://replay.pokemonshowdown.com";
const NUM_CPUS: usize = 4;

/// JSON representation of the replays sent out as a response.
#[derive(Serialize)]
struct Replays {
    replays: Vec<String>,
}

/// Async HTTP service for retrieving recent Pokemon Showdown replays.
struct ShowdownReplayService {
    /// The async HTTP client for retrieving the
    /// Pokemon Showdown replay page.
    client: Client<HttpConnector>,
    /// Thread pool for running scraping work.
    pool: Rc<CpuPool>,
}

impl ShowdownReplayService {
    fn new(handle: Handle) -> ShowdownReplayService {
        ShowdownReplayService {
            client: Client::new(&handle),
            pool: Rc::new(CpuPool::new(NUM_CPUS)),
        }
    }
}

impl Service for ShowdownReplayService {
    type Request = Request;
    type Response = Response;
    type Error = Error;
    type Future = Box<Future<Item = Response, Error = Error>>;

    fn call(&self, req: Self::Request) -> Self::Future {
        match (req.method(), req.path()) {
            (&Get, "/") => {
                let url = Url::parse(REPLAY_URL).unwrap();
                let pool = self.pool.clone();
                let get_replays = self.client.get(url).and_then(|res| {
                    res.body()
                        // Collect the body chunks into a string.
                        .fold(vec![], |mut body, chunk| {
                            body.extend_from_slice(&chunk);
                            Ok::<_, hyper::Error>(body)
                        })
                        // Scrape the body on another thread.
                        .and_then(move |body| {
                            let body = String::from_utf8(body).unwrap();
                            pool.spawn_fn(|| future::ok(scrape_replays(body)))
                        })
                        // Return a response with JSON of the replay links.
                        .and_then(|response_body| {
                            let content_len = ContentLength(response_body.len() as u64);
                            future::ok(Response::new()
                                .with_header(content_len)
                                .with_body(response_body))
                        })
                });

                Box::new(get_replays)
            }
            _ => future::ok(Response::new().with_status(StatusCode::NotFound)).boxed(),
        }
    }
}

/// Retrieves all of the recent replays given the body to the replay site.
fn scrape_replays(body: String) -> String {
    let document = kuchiki::parse_html().one(body);
    let selector = ".linklist";
    let mut replays = vec![];

    // First select the second replays list element
    // (the first replay list is for featured replays).
    if let Ok(ul) = document.select(selector).and_then(|mut m| m.nth(1).ok_or(())) {
        let ul = ul.as_node();
        let selector = "li>a";

        // Then select all links embedded inside list children.
        if let Ok(matches) = ul.select(selector) {
            for css_match in matches {
                let node = css_match.as_node();

                // Get the href attribute from the link and add it to replays.
                if let Some(elem) = node.as_element() {
                    let attrs = elem.attributes.borrow();
                    if let Some(href) = attrs.get("href") {
                        let full_link = format!("{}{}", REPLAY_URL, href);
                        replays.push(full_link);
                    }
                }
            }
        }
    }

    let replays = Replays { replays: replays };
    serde_json::to_string(&replays).unwrap()
}

fn main() {
    let addr = "127.0.0.1:1337".parse().unwrap();
    let http = Http::new();

    let mut lp = Core::new().expect("Error creating event loop");
    let handle = lp.handle();
    let service_handle = handle.clone(); // Handle to pass into the service.
    let listener = TcpListener::bind(&addr, &handle).expect("Error binding address");

    println!("Listening on http://{}", listener.local_addr().unwrap());

    let service_factory = move || ShowdownReplayService::new(service_handle.clone());
    // Create a service to handle every new connection.
    let server = listener.incoming().for_each(move |(socket, addr)| {
        http.bind_connection(&handle, socket, addr, service_factory());
        Ok(())
    });

    lp.run(server).expect("Error running the server");
}
