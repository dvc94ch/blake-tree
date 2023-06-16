FROM scratch
COPY bin/peershare peershare
COPY static static
ENTRYPOINT ["/peershare"]