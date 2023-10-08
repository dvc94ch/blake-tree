#!/bin/sh

# perform ocr on png
./scraper.py ../assets/public_domain_logo.png public_domain_logo.txt
# extract text from pdf
./scraper.py ../assets/the-public-domain.pdf /tmp/the-public-domain.txt
# transcribe video
./scraper.py ../assets/big-buck-bunny_trailer.webm /tmp/big-buck-bunny_trailer.txt

peershare-cli create ../assets/pg71819.txt
peershare-cli create ../assets/public_domain_logo.png --content /tmp/public_domain_logo.txt
peershare-cli create ../assets/the-public-domain.pdf --content /tmp/the-public-domain.txt
peershare-cli create ../assets/big-buck-bunny_trailer.webm --content /tmp/big-buck-bunny_trailer.txt
