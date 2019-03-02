extern crate html5ever;
extern crate reqwest;
#[macro_use]
extern crate log;
extern crate env_logger;

use std::collections::{HashSet, VecDeque};
use std::default::Default;
use std::sync::mpsc::{Receiver, Sender};

use html5ever::rcdom::{Handle, NodeData, RcDom};
use html5ever::{parse_document, ParseOpts};

use html5ever::tendril::TendrilSink;

// Extracts the value of links recursively from the HTML node passed as an argument
fn extract_links(node: Handle, acc: &mut Vec<String>) -> &Vec<String> {
    match node.data {
        NodeData::Element {
            ref name,
            ref attrs,
            ..
        } if name.local.to_string() == "a" => {
            attrs
                .borrow()
                .iter()
                .filter(|attr| attr.name.local.to_string() == "href")
                .map(|attr| attr.value.to_string())
                .map(|link| {
                    if link.starts_with("//") {
                        let l: String = link.trim_start_matches("/").into();
                        return format!("https://{}", l);
                    }
                    link
                })
                .filter(|link| link.starts_with("http"))
                .last()
                .map(|link| acc.push(link));
        }
        // If its not a link we do not care about the element
        _ => {}
    };

    // Recursively extract all the links in the document
    for child in node.children.borrow().iter() {
        extract_links(child.clone(), acc);
    }
    acc
}

fn main() {
    env_logger::init();

    let mut visited: HashSet<String> = HashSet::new();

    // Used a VecDeque for simple round-robin scheduling by appending the worker at the back and picking the next workir from the front
    let mut workers: VecDeque<Worker> = VecDeque::with_capacity(10);

    let (sender, receiver): (Sender<MasterMsg>, Receiver<MasterMsg>) = std::sync::mpsc::channel();

    for id in 0..10 {
        let worker = Worker::start(id);
        workers.push_back(worker);
    }

    info!("All workers initialized");

    let worker = workers.back().unwrap();
    worker.process(
        "https://en.wikipedia.org/wiki/Wikipedia:Wiki_Game".into(),
        sender.clone(),
    );

    let mut processed = 0;
    loop {
        let msg = receiver.recv().unwrap();
        processed = processed + 1;
        match msg {
            MasterMsg::Found(links) => {
                for link in links.iter() {
                    if link == "https://en.wikipedia.org/wiki/Adolf_Hitler" {
                        println!("Found it!");
                        std::process::exit(0);
                    }
                    if !visited.contains(link) {
                        visited.insert(link.clone());
                        let worker = workers.pop_front().unwrap();
                        worker.process(link.clone(), sender.clone());
                        workers.push_back(worker);
                    }
                }
            }
        }
    }
}

struct Worker {
    sender: Sender<WorkerMsg>,
}

enum WorkerMsg {
    Process(String, Sender<MasterMsg>),
}

enum MasterMsg {
    Found(Vec<String>),
}

impl Worker {
    fn start(id: i32) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            info!("Worker {} initialized", id);
            loop {
                receiver
                    .recv()
                    .map(|msg| {
                        match msg {
                            WorkerMsg::Process(link, sender) => {
                                reqwest::get(&link)
                                    .map(|mut response| {
                                        let parse_opts = ParseOpts {
                                            ..Default::default()
                                        };

                                        let dom: RcDom =
                                            parse_document(RcDom::default(), parse_opts)
                                                .from_utf8()
                                                .read_from(&mut response)
                                                .unwrap();
                                        let mut links = Vec::new();
                                        extract_links(dom.document, &mut links);
                                        sender.send(MasterMsg::Found(links)).unwrap()
                                    })
                                    .unwrap_or_else(|e| {
                                        debug!("Worker {} - {}", id, link);
                                        error!("Worker {} - {}", id, e);
                                    });

                                // println!("{}", resp.text().unwrap());
                            }
                        };
                    })
                    .unwrap();
            }
        });

        Worker { sender }
    }

    fn process(&self, link: String, sender: Sender<MasterMsg>) {
        self.sender.send(WorkerMsg::Process(link, sender)).unwrap();
    }
}
