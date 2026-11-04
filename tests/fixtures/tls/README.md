# Local TLS Test Fixtures

`localhost-cert.pem` and `localhost-key.pem` are a certificate and publicly
committed **test-only key**, signed by `root-cert.pem`. They are not deployment
credentials and must never be used outside local tests. The certificate covers
`localhost` and `127.0.0.1`; production clients must reject its unknown issuer.
The CA private key is discarded when generating the fixtures.

Regenerate before the certificate expires, then run the HTTP pool tests:

```bash
set -eu
tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes \
  -keyout "$tmpdir/ca-key.pem" -out tests/fixtures/tls/root-cert.pem \
  -days 3650 -subj '/CN=my-media-sub test CA only' \
  -addext 'basicConstraints=critical,CA:TRUE' \
  -addext 'keyUsage=critical,keyCertSign,cRLSign'
openssl req -new -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes \
  -keyout tests/fixtures/tls/localhost-key.pem \
  -out "$tmpdir/localhost.csr" -subj '/CN=my-media-sub local test only'
printf '%s\n' 'subjectAltName=DNS:localhost,IP:127.0.0.1' \
  'basicConstraints=critical,CA:FALSE' 'keyUsage=critical,digitalSignature' \
  'extendedKeyUsage=serverAuth' > "$tmpdir/extensions.cnf"
openssl x509 -req -in "$tmpdir/localhost.csr" \
  -CA tests/fixtures/tls/root-cert.pem -CAkey "$tmpdir/ca-key.pem" \
  -set_serial 2 -out tests/fixtures/tls/localhost-cert.pem \
  -days 3650 -extfile "$tmpdir/extensions.cnf"
cargo test --locked clients::http_pool::tests
```
