# c6ol: Connect6 Online

A web app for playing [Connect6] games online.

[Connect6]: https://en.wikipedia.org/wiki/Connect6

## Setup

Install Node.js v20.14+ and nightly Rust.

```sh
git clone https://github.com/yescallop/c6ol
cd c6ol/client
npm install
npm run build
cd ../server
cargo run
```

The app will be served on port 8086.

## Play

You can choose to play offline or online.

An offline game is saved in the browser's local storage.

To start an online game as Black, you submit a passcode.
You get a game link or ID to send to your opponent.
They submit a different passcode to join the game as White.

For now, a game will end unsaved if no one is connected to it.
