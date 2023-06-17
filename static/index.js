async function fetchStreams() {
  const response = await fetch("/streams");
  const ids = await response.json();
  const streams = await Promise.all(ids.map(async function(id) {
    const metadata = await fetch(`/streams/${id}`, {method: "HEAD"});
    const mime = metadata.headers.get("content-type");
    const length = metadata.headers.get("content-length");
    const url = await streamUrl(id, mime);
    return {'id': id, 'mime': mime, 'length': length, 'url': url};
  }));
  const table = document.querySelector("#streams");
  streams.forEach(function(stream) {
    const id = stream['id'];
    const mime = stream['mime'];
    const length = stream['length'];
    const url = stream['url'];
    table.innerHTML +=
      `<tr><td><a href="${url}">${id}</a></td><td>${mime}</td><td>${length}</td></tr>`;
  });
}

async function streamUrl(stream, mime) {
  if (mime == "application/x-peershare") {
    const response = await fetch(`/streams/${stream}`);
    const manifest = await response.json();
    const id =  manifest['streamId'];
    const metadata = await fetch(`/streams/${id}`, {method: "HEAD"});
    const mime = metadata.headers.get("content-type");
    return await streamUrl(id, mime);
  }
  if (mime == "application/dash+xml") {
    return `/player?stream=${stream}`;
  }
  return `/streams/${stream}`;
}