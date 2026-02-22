# tcpprepend
Simple TCP forwarder that prepends some fixed bytes to responses and ignores some fixed header on requests

Install it by building (`cargo install --path .`) or download pre-built executables from [Github Releases](https://github.com/Bonveio/tcpprepend/releases/).

## Example

```
A$ echo -n 'ABC' | base64
A: QUJD

A$ echo -n 'XYZ' | base64
A: WFla

A$ tcpprepend 127.0.0.1:1234 QUJD 127.0.0.1:1235 WFla&
A: [1] 30101

B$ nc -lvp 1235
B: Listening on 0.0.0.0 1235

C$ nc 127.0.0.1 1234
A: Incoming connection from 127.0.0.1:40834
C> 12345
C> asdfg
C> 67ABC890
A:  found matching request bytes
A:   connected to upstream
A:   wrote prepender bytes
B: Connection received on localhost 44242
B: 890
A: XYZ
C> tttyyy
B: tttyyy
B> 555666
C: 555666
```

Pseudo-HTTP server mode:
```
  $ tcpprepend 127.0.0.1:8080 DQoNCg== 127.0.0.1:1235 SFRUUC8xLjAgMjAwIE9LDQoNCg==
  $ curl http://127.0.0.1:8080/
```

Websocket:
```
  $ printf "%s" "HTTP/1.1 101 Switching Protocols/r/n/r/n" | base64
  SFRUUC8xLjEgMTAxIFN3aXRjaGluZyBQcm90b2NvbHMvci9uL3Ivbg==
  $ tcpprepend 0.0.0.0:80 DQoNCg== 127.0.0.1:22 SFRUUC8xLjEgMTAxIFN3aXRjaGluZyBQcm90b2NvbHMvci9uL3Ivbg==
```

HTTP proxy (200 Connection Established):
```
  $ tcpprepend 0.0.0.0:8080 DQoNCg== 127.0.0.1:2424 SFRUUC8xLjEgMjAwIENvbm5lY3Rpb24gRXN0YWJsaXNoZWQNCg0K
```

### Usage line
```
ARGS:
  <listen>

  <request_needle_base64>

  <connect>

  <response_prepend_base64>
```
