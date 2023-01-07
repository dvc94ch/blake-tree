use blake_tree::{Mime, Range, Stream, StreamId, StreamStorage};
use std::io::Read;
use std::str::FromStr;
use std::sync::Arc;
use tide::{Body, Response};

pub async fn server(store: StreamStorage) -> tide::Server<Arc<StreamStorage>> {
    let mut app = tide::with_state(Arc::new(store));
    app.at("/").get(list);
    app.at("/").post(add);
    app.at("/:id").get(read);
    app.at("/:id").delete(remove);
    app.at("/:id/ranges").post(ranges);
    app.at("/:id/missing_ranges").post(missing_ranges);
    app
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

async fn read(req: Request) -> tide::Result {
    let stream = stream(&req)?;
    let range = if let Some(values) = req.header(tide::http::headers::CONTENT_RANGE) {
        from_content_range(values.get(0).unwrap().as_str())?
    } else {
        stream.id().range()
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
    Ok(Response::builder(200)
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

fn from_content_range(range: &str) -> Result<Range, tide::Error> {
    let (unit, rest) = range.split_once(' ').ok_or_else(invalid_content_range)?;
    if unit != "bytes" {
        return Err(invalid_content_range());
    }
    let (range, length) = rest.split_once('/').ok_or_else(invalid_content_range)?;
    let length: u64 = length.parse().map_err(|_| invalid_content_range())?;
    let (start, end) = range.split_once('-').ok_or_else(invalid_content_range)?;
    let start: u64 = start.parse().map_err(|_| invalid_content_range())?;
    let end: u64 = end.parse().map_err(|_| invalid_content_range())?;
    if end - start != length {
        return Err(invalid_content_range());
    }
    Ok(Range::new(start, length))
}

fn invalid_content_range() -> tide::Error {
    tide::Error::new(400, anyhow::anyhow!("invalid content-range"))
}

fn to_content_range(range: &Range) -> String {
    format!(
        "bytes {}-{}/{}",
        range.offset(),
        range.end(),
        range.length()
    )
}
