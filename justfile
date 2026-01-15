fmt:
    make fmt

lint:
    make lint

test:
    make test

ci:
    just fmt && just lint && just test

coverage:
    make coverage

doc:
    make doc