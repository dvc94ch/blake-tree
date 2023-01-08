use anyhow::{Context, Result};
use blake_tree::{Mime, Range, Stream, StreamId, StreamStorage};
use futures::io::BufReader;
use std::io::Read;
use std::str::FromStr;
use std::sync::Arc;
use tide::http::headers::HeaderName;
use tide::security::{CorsMiddleware, Origin};
use tide::{Body, Response};

pub async fn server(store: StreamStorage) -> tide::Server<Arc<StreamStorage>> {
    let mut app = tide::with_state(Arc::new(store));
    app.at("/").get(list);
    app.at("/").post(add);
    app.at("/:id").head(length);
    app.at("/:id").get(read);
    app.at("/:id").delete(remove);
    app.at("/:id/ranges").get(ranges);
    app.at("/:id/missing-ranges").get(missing_ranges);
    app
}

pub async fn blake_tree_http(store: StreamStorage, url: String) -> Result<()> {
    let server = server(store).await;

    let cors = CorsMiddleware::new()
        .allow_origin(Origin::from("*"))
        .allow_credentials(false);

    let mut app = tide::new();
    app.with(tide::log::LogMiddleware::new());
    app.with(cors);
    app.at("/").nest(server);
    app.listen(&url)
        .await
        .with_context(|| format!("listening on {}", &url))?;
    Ok(())
}

type Request = tide::Request<Arc<StreamStorage>>;

async fn list(req: Request) -> tide::Result {
    let store = req.state();
    let streams = store.streams().collect::<Vec<_>>();
    Ok(Response::builder(200)
        .body(Body::from_json(&streams)?)
        .build())
}

async fn add(mut req: Request) -> tide::Result {
    let mime = if let Some(mime) = req.content_type() {
        Mime::from_mime(mime.essence())
            .ok_or_else(|| tide::Error::new(400, anyhow::anyhow!("unsupported mime type")))?
    } else {
        Mime::default()
    };
    let body = req.body_bytes().await?;
    let store = req.state();
    let stream = store.insert(mime, &mut &body[..])?;
    Ok(Response::builder(200)
        .body(Body::from_json(stream.id())?)
        .build())
}

async fn length(req: Request) -> tide::Result {
    let id = stream_id(&req)?;
    let empty = BufReader::new(futures::io::empty());
    let mut body = Body::from_reader(empty, Some(id.length() as _));
    let mime = id.mime().mime();
    body.set_mime(tide::http::Mime::from_str(mime).unwrap());
    Ok(Response::builder(200)
        .header(tide::http::headers::ACCEPT_RANGES, "bytes")
        .body(body)
        .build())
}

async fn read(req: Request) -> tide::Result {
    let stream = stream(&req)?;
    let (range, status) = if let Some(values) = req.header(HeaderName::from("Range")) {
        log::info!("Range: {}", values);
        (from_range(values.get(0).unwrap().as_str())?, 206)
    } else {
        (stream.id().range(), 200)
    };
    let mut reader = stream
        .read_range(range)
        .map_err(|err| tide::Error::new(500, err))?;
    let mut bytes = Vec::with_capacity(range.length() as _);
    reader
        .read_to_end(&mut bytes)
        .map_err(|err| tide::Error::new(500, err))?;
    let mut body = Body::from_bytes(bytes);
    let mime = stream.id().mime().mime();
    body.set_mime(tide::http::Mime::from_str(mime).unwrap());
    Ok(Response::builder(status)
        .header(tide::http::headers::CONTENT_RANGE, to_content_range(&range))
        .body(body)
        .build())
}

async fn ranges(req: Request) -> tide::Result {
    let stream = stream(&req)?;
    let ranges = stream.ranges().map_err(|err| tide::Error::new(500, err))?;
    Ok(Response::builder(200)
        .body(Body::from_json(&ranges)?)
        .build())
}

async fn missing_ranges(req: Request) -> tide::Result {
    let stream = stream(&req)?;
    let missing_ranges = stream
        .missing_ranges()
        .map_err(|err| tide::Error::new(500, err))?;
    Ok(Response::builder(200)
        .body(Body::from_json(&missing_ranges)?)
        .build())
}

async fn remove(req: Request) -> tide::Result {
    let id = stream_id(&req)?;
    let store = req.state();
    store
        .remove(&id)
        .map_err(|err| tide::Error::new(500, err))?;
    Ok(Response::builder(200).build())
}

fn stream_id(req: &Request) -> Result<StreamId, tide::Error> {
    let id = req
        .param("id")?
        .parse()
        .map_err(|err| tide::Error::new(400, err))?;
    let store = req.state();
    if !store.contains(&id) {
        return Err(tide::Error::new(404, anyhow::anyhow!("stream not found")));
    }
    Ok(id)
}

fn stream(req: &Request) -> Result<Stream, tide::Error> {
    let id = stream_id(req)?;
    let store = req.state();
    store.get(&id).map_err(|err| tide::Error::new(500, err))
}

fn from_range(range: &str) -> Result<Range, tide::Error> {
    let (unit, range) = range.split_once('=').ok_or_else(invalid_range)?;
    if unit != "bytes" {
        return Err(invalid_range());
    }
    let (start, end) = range.split_once('-').ok_or_else(invalid_range)?;
    let start: u64 = start.parse().map_err(|_| invalid_range())?;
    let end: u64 = end.parse().map_err(|_| invalid_range())?;
    let length = end.checked_sub(start).ok_or_else(invalid_range)?;
    Ok(Range::new(start, length))
}

fn invalid_range() -> tide::Error {
    tide::Error::new(400, anyhow::anyhow!("invalid range"))
}

fn to_content_range(range: &Range) -> String {
    format!(
        "bytes {}-{}/{}",
        range.offset(),
        range.end(),
        range.length()
    )
}
