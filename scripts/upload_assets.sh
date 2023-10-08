#!/bin/sh

echo "perform ocr on png"
./scraper.py ../assets/public_domain_logo.png /tmp/public_domain_logo.txt
echo "extract text from pdf"
./scraper.py ../assets/the-public-domain.pdf /tmp/the-public-domain.txt
echo "transcribe video"
./scraper.py ../assets/big-buck-bunny_trailer.webm /tmp/big-buck-bunny_trailer.txt

echo "uploading files"
peershare-cli create ../assets/pg71819.txt
peershare-cli create ../assets/public_domain_logo.png --content /tmp/public_domain_logo.txt
peershare-cli create ../assets/the-public-domain.pdf --content /tmp/the-public-domain.txt
peershare-cli create ../assets/big-buck-bunny_trailer.webm --content /tmp/big-buck-bunny_trailer.txt
