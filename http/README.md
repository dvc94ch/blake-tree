# peershare-http

## List streams (GET /streams)
```
curl http://127.0.0.1:3000/streams
["AGbP8Ns5JCMucflZKtyqF-i3wmlWBONmf1LH1-vIyzWg7wQAAAAAAAAmAA=="]
```

## Create stream (POST /streams)
```
curl -d @/tmp/f -H "Content-Type: application/octet-stream" http://127.0.0.1:3000/streams
"ALc65rWQ41E9B2VbeW_HyDfZ508Sl3ryKezYZElU9O3iAQgAAAAAAAAAAA=="
```

## Stream metadata (HEAD /streams/:id)
```
curl -I http://127.0.0.1:3000/streams/AGbP8Ns5JCMucflZKtyqF-i3wmlWBONmf1LH1-vIyzWg7wQAAAAAAAAmAA==
HTTP/1.1 200 OK
accept-ranges: bytes
content-length: 1263
content-type: text/plain
```

## Read stream (GET /streams/:id)
```
curl -r 0-15  http://127.0.0.1:3000/streams/AGbP8Ns5JCMucflZKtyqF-i3wmlWBONmf1LH1-vIyzWg7wQAAAAAAAAmAA==
The blockchain
```

## List ranges (GET /streams/:id/ranges)
```
curl http://127.0.0.1:3000/streams/AGbP8Ns5JCMucflZKtyqF-i3wmlWBONmf1LH1-vIyzWg7wQAAAAAAAAmAA==/ranges
[{"offset":0,"length":1263}]
```

## List missing ranges (GET /streams/:id/missing-ranges)
```
curl http://127.0.0.1:3000/streams/AGbP8Ns5JCMucflZKtyqF-i3wmlWBONmf1LH1-vIyzWg7wQAAAAAAAAmAA==/missing-ranges
[]
```

## Delete stream (DELETE /streams/:id)
```
curl -X delete http://127.0.0.1:3000/streams/AMCk9GOQlj1qcwjsUVSxFruK2TARfeUbVYZXYH3MgGatBgAAAAAAAAAmAA==
```
