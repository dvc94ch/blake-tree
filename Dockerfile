FROM scratch
COPY bin/peershare peershare
ENTRYPOINT ["/peershare"]