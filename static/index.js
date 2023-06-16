async function fetchStreams() {
  const response = await fetch("/streams");
  const streams = await response.json();
  const table = document.querySelector("#streams");
  streams.forEach(async function(stream) {
    const metadata = await fetch(`/streams/${stream}`, {method: "HEAD"});
    const mime = metadata.headers.get("content-type");
    const length = metadata.headers.get("content-length");
    table.innerHTML +=
      `<tr><td><a href="/streams/${stream}">${stream}</a></td><td>${mime}</td><td>${length}</td></tr>`;
  });
}