#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::{vec, fs::File, io::Write, env, path::Path};
use std::error::Error;

use reqwest;
use scraper::{Html, Selector};
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};
use epub_builder::{EpubBuilder, ZipLibrary, EpubContent, ReferenceType};

static FETCH_LIST_MUTEX:Mutex<i32> = Mutex::const_new(0);
static DOWNLOAD_MUTEX:Mutex<i32> = Mutex::const_new(0);

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![download, fetch_ranklist, search])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[derive(Serialize)]
struct Response<T> {
    status_code: i32,
    data: T,
    message: String,
}

#[derive(Serialize, Deserialize, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Episode {
    title: String,
    url: String,
    content: String,
    number: i32,
}

#[derive(Serialize, Deserialize, Debug)]
struct Novel {
    name: String,
    author: String,
    url: String,
    eps: Vec<Episode>,
}

#[tauri::command]
async fn fetch_ranklist(novel_type: String, rank_time: String) -> Response<Vec<Novel>> {
    match FETCH_LIST_MUTEX.try_lock() {
        Ok(_lock) => {
            match _fetch_ranklist(novel_type, rank_time).await {
                Ok(content) => {
                    Response{status_code: 0, data: content, message: String::from("")}
                }
                Err(err) => {
                    Response{status_code: 1, data: vec![], message: err.to_string() }
                }
            }
        }
        Err(err) => {
            Response{status_code: 2, data: vec![], message: err.to_string() }
        }
    }
}

#[tauri::command]
async fn search(query: String) -> Response<Vec<Novel>> {
    match FETCH_LIST_MUTEX.try_lock() {
        Ok(_lock) => {
            match _search(query).await {
                Ok(content) => {
                    Response{status_code: 0, data: content, message: String::from("")}
                }
                Err(err) => {
                    Response{status_code: 1, data: vec![], message: err.to_string() }
                }
            }
        }
        Err(err) => {
            Response{status_code: 2, data: vec![], message: err.to_string() }
        }
    }
}

#[tauri::command]
async fn download(novel: Novel) -> String {
    // println!("{:?}", novel);
    match DOWNLOAD_MUTEX.try_lock() {
        Ok(_) => { 
            let _novel: Novel;
            match _download(&novel).await {
                Ok(option_novel) => {
                    match option_novel {
                        None => { return String::from("failed to download novel"); }
                        Some(novel) => {
                            _novel = novel;
                        }
                    }
                }
                Err(err) => { return err.to_string(); }
            }
            match _export_epub(_novel).await {
                Ok(_) => { String::new() }
                Err(err) => { err.to_string() }
            }
        }
        Err(err) => { err.to_string() }
    }
}

// fetch monthly ranlist by type
async fn _fetch_ranklist(novel_type: String, rank_time: String) -> Result<Vec<Novel>, Box<dyn Error>> {
    let resp = reqwest::get(format!("https://kakuyomu.jp/rankings/{novel_type}/{rank_time}")).await?;
    if resp.status() != reqwest::StatusCode::OK {
        return Err(resp.error_for_status().err().unwrap().to_string().into());
    }
    let content = resp.text().await?;
    _parse_document(content)
}

async fn _search(query: String) -> Result<Vec<Novel>, Box<dyn Error>> {
    let resp = reqwest::get(format!("https://kakuyomu.jp/search?q={query}&order=popular")).await?;
    if resp.status() != reqwest::StatusCode::OK {
        return Err(resp.error_for_status().err().unwrap().to_string().into());
    }
    let content = resp.text().await?;
    _parse_document(content)
}

fn _parse_document(doc: String)-> Result<Vec<Novel>, Box<dyn Error>>  {
    let doc = Html::parse_document(&doc);
    let novel_selector = Selector::parse(".widget-work").unwrap();
    let title_selector = Selector::parse(".widget-workCard-titleLabel").unwrap();
    let authror_selector = Selector::parse(".widget-workCard-authorLabel").unwrap();
    let mut novels = Vec::new();
    for el in doc.select(&novel_selector) {
        let title_el = el.select(&title_selector).last().unwrap();
        let author_el = el.select(&authror_selector).last().unwrap();
        let name = title_el.inner_html();
        let href = title_el.value().attr("href").unwrap().to_string();
        let author = author_el.inner_html();
        novels.push(Novel{
            url: format!("https://kakuyomu.jp{href}"),
            author,
            name,
            eps: vec![],
        });
    }
    Ok(novels)
}

async fn _download(novel: &Novel) -> Result<Option<Novel>, Box<dyn Error>> {
    let filename = format!("{}.epub", novel.name);
    let path = Path::new(&filename);
    if path.exists() {
        let message = format!("{} is already exists", &filename);
        return Err(message.into());
    }
    let novel_page_resp = reqwest::get(&novel.url).await?;
    if novel_page_resp.status() != reqwest::StatusCode::OK {
        return Err(Box::from(novel_page_resp.error_for_status().err().unwrap() ));

    }
    let mut thread_vec = Vec::new();
    let (sx, mut rx) = mpsc::channel(10);
    {
        // fetch novel detail page
        let content = novel_page_resp.text().await?;
        let doc = Html::parse_document(&content);
        let ep_selector = Selector::parse(".widget-toc-episode-episodeTitle").unwrap();
        // let title_selector = Selector::parse(".widget-toc-episode-titleLabel").unwrap();
        
        // downlaod novel by chapter
        let mut number = 0;
        for ep_el in doc.select(&ep_selector) {
            number += 1;
            let tsx = sx.clone();
            let url = ep_el.value().attr("href").unwrap().to_string();
            let url = format!("https://kakuyomu.jp{url}");
            // let title = ep_el.select(&title_selector).last().unwrap().inner_html();
            thread_vec.push(tokio::spawn(async move {
                let mut retry = 5;
                let mut ok = false;
                while retry > 0 && !ok {
                    let url = url.clone();
                    match reqwest::get(&url).await {
                        Ok(ep_page_resp) => {
                            if ep_page_resp.status() != reqwest::StatusCode::OK {
                                return Err(ep_page_resp.error_for_status().err().unwrap());
                            }
                            let content = ep_page_resp.text().await?;
                            let ep: Episode;
                            {
                                let doc = Html::parse_document(&content);
                                let main_selector = Selector::parse(".widget-episode").unwrap();
                                let title_selector = Selector::parse(".widget-episodeTitle").unwrap();
                                let content = doc.select(&main_selector).last().unwrap().html();
                                let title = doc.select(&title_selector).last().unwrap().inner_html();
                                let content = _make_content(content,title.clone());
                                ep = Episode{title, content, url, number}
                            }
                            tsx.send(ep).await.unwrap();
                            ok = true;
                        }
                        Err(err) => { 
                            retry -= 1; 
                            if retry < 1 { 
                                return Err(err) 
                            }
                        }
                    }
                }
                Ok::<u8, reqwest::Error>(0)
            }));
        }
    }
    // the thread would pending here if we don't drop the origin sender.
    drop(sx);
    let mut ep_vec = vec![];
    while let Some(ep) = rx.recv().await {
        ep_vec.push(ep);
    }
    for thread in thread_vec {
        thread.await.unwrap()?;
    }
    ep_vec.sort_by(|a, b| a.number.cmp(&b.number));
    let res = Novel { name: novel.name.clone(), author: novel.author.clone(), url: novel.url.clone(), eps: ep_vec };
    Ok(Some(res))
}

async fn _export_epub(novel: Novel) -> Result<(), Box<dyn Error>> {
    let zip_library = ZipLibrary::new()?;
    let mut epub = EpubBuilder::new(zip_library)?;
    let mut epub = epub.metadata("author", novel.author.as_str())?
        .metadata("title", novel.name.as_str())?
        .metadata("lang", "ja-JP")?
        .stylesheet(CSS.as_bytes())?;
    for ep in novel.eps {
        epub = epub.add_content(
        EpubContent::new(format!("ep{}.xhtml", ep.number), 
                         ep.content.as_bytes()).title(ep.title).reftype(ReferenceType::Text)
        )?;
    }
    epub = epub.inline_toc();
    let mut data: Vec<u8> = vec![];
    epub.generate(&mut data)?;
    let filename = format!("{}.epub", novel.name);
    let mut fp = match File::open(&filename) {
        Ok(fp) => { fp }
        Err(_) => {
            match File::create(&filename) {
                Ok(fp) => { fp }
                Err(_) => { panic!("cannot open file!") }
            }
        }
    };
    fp.write(data.as_slice()).unwrap();
    Ok(())
}

fn _make_content(content: String, title: String) -> String {
    let content = content.replace("<br>", "<br />");
    format!("<?xml version=\"1.0\" encoding=\"utf-8\"?> 
            <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" xml:lang=\"zh-CN\" xmlns:xml=\"http://www.w3.org/XML/1998/namespace\">
            <head><link href=\"stylesheet.css\" type=\"text/css\" rel=\"stylesheet\" /></head>
            <body><h2>{title}</h2>{content}</body>
            </html>")
}

const CSS: &str = "
body{padding: 0%;margin-top: 0%;margin-bottom: 0%;margin-left: 1%;margin-right: 1%;line-height:1.2;text-align: justify;}
p {text-indent:2em;display:block;line-height:1.3;margin-top:0.6em;margin-bottom:0.6em;}
";

