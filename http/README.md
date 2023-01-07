# blake-tree-http

## List streams (GET /)
```
curl http://127.0.0.1:3000/
["AGbP8Ns5JCMucflZKtyqF-i3wmlWBONmf1LH1-vIyzWg7wQAAAAAAAAmAA=="]
```

## Create stream (POST /)
```
curl -d @/tmp/f -H "Content-Type: application/octet-stream"  http://127.0.0.1:3000/
"ALc65rWQ41E9B2VbeW_HyDfZ508Sl3ryKezYZElU9O3iAQgAAAAAAAAAAA=="
```

## Stream metadata (HEAD /:id)
```
curl -I http://127.0.0.1:3000/AGbP8Ns5JCMucflZKtyqF-i3wmlWBONmf1LH1-vIyzWg7wQAAAAAAAAmAA==
HTTP/1.1 200 OK
accept-ranges: bytes
content-length: 1263
content-type: text/plain
```

## Read stream (GET /:id)
```
curl -r 0-15  http://127.0.0.1:3000/AGbP8Ns5JCMucflZKtyqF-i3wmlWBONmf1LH1-vIyzWg7wQAAAAAAAAmAA==
The blockchain
```

## List ranges (GET /:id/ranges)
```
curl http://127.0.0.1:3000/AGbP8Ns5JCMucflZKtyqF-i3wmlWBONmf1LH1-vIyzWg7wQAAAAAAAAmAA==/ranges
[{"offset":0,"length":1263}]
```

## List missing ranges (GET /:id/missing-ranges)
```
curl http://127.0.0.1:3000/AGbP8Ns5JCMucflZKtyqF-i3wmlWBONmf1LH1-vIyzWg7wQAAAAAAAAmAA==/missing-ranges
[]
```

## Delete stream (DELETE /:id)
```
curl -X delete http://127.0.0.1:3000/AMCk9GOQlj1qcwjsUVSxFruK2TARfeUbVYZXYH3MgGatBgAAAAAAAAAmAA==
```
