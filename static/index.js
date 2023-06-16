async function fetchStreams() {
  const response = await fetch("/streams");
  const ids = await response.json();
  const streams = await Promise.all(ids.map(async function(id) {
    const metadata = await fetch(`/streams/${id}`, {method: "HEAD"});
    const mime = metadata.headers.get("content-type");
    const length = metadata.headers.get("content-length");
    return {'id': id, 'mime': mime, 'length': length};
  }));
  const table = document.querySelector("#streams");
  streams.forEach(function(stream) {
    const id = stream['id'];
    const mime = stream['mime'];
    const length = stream['length'];
    const url = streamUrl(id, mime);
    table.innerHTML +=
      `<tr><td><a href="${url}">${id}</a></td><td>${mime}</td><td>${length}</td></tr>`;
  });
}

function streamUrl(stream, mime) {
  if (mime == "application/dash+xml") {
    return `/player?stream=${stream}`;
  }
  return `/streams/${stream}`;
}