# c6ol: Connect6 Online

A web app for playing [Connect6](https://en.wikipedia.org/wiki/Connect6) games online.
Currently provides a single shared board with no authentication whatsoever.

## Prerequisites

Node.js v20.17.0. Go 1.23.1.

## How to run and play

```sh
git clone https://github.com/yescallop/c6ol
cd c6ol/client
npm install
npm run build
cd ../server
go run .
```

The app will be served on port 8086. Use `-addr` to specify the address to listen to.
