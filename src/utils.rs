use std::{fs, io};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::process::exit;
use std::str::from_utf8;
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use xml::EventReader;
use xml::reader::XmlEvent;

pub(crate) type TF = HashMap<String, usize>;
pub(crate) type TFIndex = HashMap<PathBuf, TF>;

pub(crate) struct Lexer<'a> {
    content: &'a [char]
}

impl <'a> Lexer<'a> {
    fn new(content: &'a [char]) -> Self {
        Self {content}
    }

    fn trim_left(&mut self){
        while self.content.len() > 0 && self.content[0].is_whitespace() {
            self.content = &self.content[1..];
        }
    }

    fn chop_while<P>(&mut self, mut predicate: P) -> &'a[char] where P: FnMut(&char) -> bool {
        let mut i = 0;
        while i < self.content.len() && predicate(&self.content[i]) {
            i += 1;
        }
        return self.chop(i);
    }
    fn chop(&mut self, i: usize) -> &'a[char] {
        let token = &self.content[0..i];
        self.content = &self.content[i..];
        return token;
    }
    fn next_token(&mut self) -> Option<String> {
        self.trim_left();
        if self.content.len() == 0 {
            return None;
        }
        return if self.content[0].is_alphabetic() {
            Some(self.chop_while(|x| x.is_alphanumeric()).iter().map(|x|x.to_ascii_uppercase()).collect::<String>())
        } else if self.content[0].is_numeric() {
            Some(self.chop_while(|x| x.is_numeric()).iter().collect::<String>())
        } else {
            Some(self.chop(1).iter().collect::<String>())
        }
    }
}

impl <'a> Iterator for Lexer<'a> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

pub(crate) fn index_document(file_path: &str) -> HashMap<String, usize> {
    let content = read_xml_file(file_path).unwrap().chars().collect::<Vec<_>>();
    let mut tf = HashMap::<String, usize>::new();
    for term in Lexer::new(&content){
        if let Some(count) = tf.get_mut(&term){
            *count += 1;
        } else {
            tf.insert(term, 1);
        }
    }
    return tf;
}

pub(crate) fn read_xml_file(file_path: &str) -> io::Result<String> {
    let file = File::open(file_path)?;
    let er = EventReader::new(file);
    let mut content = String::new();
    for event in er.into_iter(){
        let event = event.unwrap();
        if let XmlEvent::Characters(text) = event {
            content.push_str(&text);
            content.push_str(" ");
        }
    }
    Ok(content)
}

pub(crate) fn save_index_to_file(directory_root_path: Vec<&str>, directory_paths: Vec<&str>, index_path: &str){
    let mut tf_index = TFIndex::new();
    for directory in directory_paths {
        let mut directory_path_args = directory_root_path.clone();
        directory_path_args.push(directory);
        let directory_path = directory_path_args.iter().collect::<PathBuf>();
        let mut total_tokens = 0;
        let dir = fs::read_dir(&directory_path).unwrap();
        let filepaths = dir.filter_map(|entry|{
            entry.ok().and_then(|e|
            e.path().file_name()
                .and_then(|n| n.to_str().map(|s| String::from(s)))
            )
        }).collect::<Vec<String>>();
        for filepath in filepaths {
            let fp_buffer = PathBuf::from_iter([&directory_path, &filepath.into()].iter());
            let fp = fp_buffer.to_str().unwrap();
            let tf = index_document(fp);
            total_tokens += tf.len();
            println!("{:?} => {} tokens", fp, tf.len());
            tf_index.insert(fp_buffer, tf);
        }
        println!("Total files parsed: {}, Total tokens found: {}", tf_index.len(), total_tokens);
    }
    let index_file = File::create(index_path).unwrap();
    serde_json::to_writer(index_file, &tf_index).expect("Saving index failed");
    println!("Saved index to {:?}", index_path);
}

pub(crate) fn get_index_from_file(index_path: &str) -> io::Result<TFIndex> {
    let mut index_file = File::open(index_path).unwrap();
    let mut index_as_str = String::new();
    index_file.read_to_string(&mut index_as_str).expect("Index parsing failed");
    let tf_index: TFIndex = serde_json::from_str(&*index_as_str).unwrap();
    Ok(tf_index)
}

pub(crate) fn tf(term: &str, document: &TF) -> f32 {
    *document.get(term).unwrap_or(&0) as f32 / document.iter().map(|(_, f)|*f).sum::<usize>() as f32
}

pub(crate) fn idf(term: &str, tfidx: &TFIndex) -> f32 {
    (tfidx.len() as f32 / tfidx.values().filter(|&tf|tf.contains_key(term)).count().max(1) as f32).log10()
}

pub(crate) fn serve_file(request: Request, file_path_args: Vec<&str>, status_code: u16) -> Result<(), ()> {
    let file_path = file_path_args.iter().collect::<PathBuf>();
    let file = File::open(file_path).unwrap();
    let response = Response::from_file(file).with_header(
        Header::from_bytes("Content-Type", "text/html").expect("Header generation failed")
    ).with_header(
        Header::from_bytes("Access-Control-Allow-Origin", "http://127.0.0.1:5000").expect("Header generation failed")
    ).with_status_code(StatusCode(status_code));
    request.respond(response).map_err(|err| {
        eprintln!("[ERR]: Could not process request => {}", err);
    })
}

pub(crate) fn serve_json(request: Request, content: Value, status_code: u16) -> Result<(), ()>{
    let response = Response::from_string(
        content.to_string()
    ).with_header(
        Header::from_bytes("Content-Type", "application/json").expect("JSON generation failed")
    ).with_header(
        Header::from_bytes("Access-Control-Allow-Origin", "http://127.0.0.1:5000").expect("Header generation failed")
    ).with_status_code(StatusCode(status_code));
    request.respond(response).map_err(|err| {
        eprintln!("[ERR]: Could not process request => {}", err);
    })
}

pub(crate) fn serve_request(mut require: Request, tfidx: &TFIndex){
    println!("Received request => Method: {}, URL: {}", require.method(), require.url());
    match (require.method(), require.url()) {
        (Method::Get, "/" | "/index.html") => {
            serve_file(require, ["static", "index.html"].to_vec(), 200).expect("[ERR]: Index Serve failed");
        }
        (Method::Post, "/api/search") => {
            let mut buffer = Vec::new();
            require.as_reader().read_to_end(&mut buffer).expect("[ERR] Body parsing failed");
            let query = from_utf8(&buffer).unwrap().chars().collect::<Vec<_>>();
            let mut rank_array = Vec::<(String, f32)>::new();
            for (path, doc) in tfidx.iter() {
                let mut rank = 0f32;
                for token in Lexer::new(&query){
                    rank += tf(&token, &doc) * idf(&token, &tfidx);
                }
                rank_array.push((path.to_str().unwrap().to_owned(), rank));
                rank_array.sort_by(|(_, r1), (_, r2) | r2.partial_cmp(r1).unwrap());
            }
            let rank_array_of_str = json!(rank_array.iter().map(|(s,c)|s.to_owned()).collect::<Vec<String>>());
            let rank_table = json!({
                "results": rank_array_of_str,
                "count": rank_array.len()
            });
            serve_json(require, rank_table, 200).expect("[ERR]: API Result serve failed");
        }
        _ => {
            serve_file(require, ["static", "404.html"].to_vec(), 404).expect("[ERR]: 404 Serve failed");
        }
    }
}

pub(crate) fn serve(address: &str, file_path: &str){
    let server = Server::http(&address).map_err(|err| {
        eprintln!("[ERR]: Could not start server => {}", err);
        exit(1);
    }).unwrap();
    let tfidx  = get_index_from_file(file_path).unwrap(); 
    println!("Started Server at http://{}", server.server_addr());
    for request in server.incoming_requests() {
        serve_request(request, &tfidx);
    }
}
