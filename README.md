# c6ol: Connect6 Online

A web app for playing [Connect6] games online, built with Rust frameworks [Leptos] and [axum].

[Connect6]: https://en.wikipedia.org/wiki/Connect6
[Leptos]: https://github.com/leptos-rs/leptos
[axum]: https://github.com/tokio-rs/axum

## Features

- **Easy Setup of Games:** Submit a passcode to start as Black. Then send a link to your opponent, who submits a different passcode to join as White.
- **Nearly Infinite Board:** The board is $2^{16}$ by $2^{16}$ in size, with drag & zoom support. In a game started near the center, you never worry about hitting the border.[^1]
- **Compact Record Format:** Based on zigzag encoding, a pairing function, and varints, the format encodes any stone placed within the central 11-by-11 area to a single byte.
- **Keyboard Control:** You can control the app with keyboard only.

[^1]: It is good sportsmanship to start near the center and to place stones near existing ones.

## Screenshots

<table>
  <tr>
    <td><img alt="Online Dialog" src="assets/online-dialog.png" /></td>
    <td><img alt="Game Play" src="assets/game-play.png" /></td>
  </tr>
  <tr>
    <td><img alt="Game Menu" src="assets/game-menu.png" /></td>
    <td><img alt="Game Menu Alt" src="assets/game-menu-alt.png" /></td>
  </tr>
</table>

## Setup

Install Rust 1.82+ and [Trunk](https://trunkrs.dev/). To develop, run:

```sh
git clone https://github.com/yescallop/c6ol
cd c6ol
# In one terminal:
cd server
cargo run
# In another:
cd client
trunk serve --open
```

To deploy, run:

```sh
cd client
trunk build --release
cd ../server
# To deploy in place:
cargo run --release -- -l [::]:8086 ../client/dist
# Or you can copy the outputs elsewhere and run:
/path/to/c6ol-server --listen [::]:8086 /path/to/dist
```

## Play

You can choose to play offline or online.
An offline game is saved in the browser's local storage.
For now, an online game will end unsaved if no one is connected to it.
