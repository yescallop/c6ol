<script setup lang="ts">
import { onMounted, ref, useTemplateRef } from 'vue';
import GameView from './components/GameView.vue';
import { Game, Move, MoveKind, Point, Stone } from './game';
import { ClientMessage, MessageKind, ServerMessage } from './protocol';
import { Base64 } from 'js-base64';

const openDialog = ref<HTMLDialogElement>();

const mainMenuDialog = useTemplateRef('main-menu-dialog');
const onlineMenuDialog = useTemplateRef('online-menu-dialog');
const joinDialog = useTemplateRef('join-dialog');

const onlineAction = ref('start');
const gameId = ref('');
const passcode = ref('');

const game = new Game();
const ourStone = ref<Stone>();
const view = useTemplateRef('view');

let ws: WebSocket | undefined;

/** Sends the message on the WebSocket connection. */
function send(msg: ClientMessage) {
  if (ws!.readyState != WebSocket.OPEN)
    return window.alert('Connection closed, please refresh the page.');
  ClientMessage.serialize(msg).then(buf => ws!.send(buf));
}

/** Saves the game to local storage. */
function save() {
  let buf = Base64.fromUint8Array(game.serialize(true));
  localStorage.setItem("game", buf);
}

function onMove(pos: [] | [Point] | [Point, Point]) {
  let move: Move;
  if (pos.length == 0) {
    move = { kind: MoveKind.Pass };
  } else {
    move = { kind: MoveKind.Stone, pos };
  }

  if (ws) {
    send({ kind: MessageKind.Move, move });
  } else {
    game.makeMove(move);
    save();

    ourStone.value = game.turn();
    view.value!.draw();
  }
}

function onUndo() {
  if (ws) {
    send({ kind: MessageKind.Retract });
  } else {
    game.undoMove();
    save();

    ourStone.value = game.turn();
    view.value!.draw();
  }
}

function onRedo() {
  if (!ws) {
    game.redoMove();
    save();

    ourStone.value = game.turn();
    view.value!.draw();
  }
}

function show(dialog: HTMLDialogElement) {
  openDialog.value = dialog;
  dialog.returnValue = '';
  dialog.showModal();
}

function onDialogClose() {
  if (!openDialog.value) return;

  let dialog = openDialog.value;

  if (dialog == mainMenuDialog.value) {
    if (dialog.returnValue == 'online')
      return show(onlineMenuDialog.value!);

    location.hash = '#local';
  } else if (dialog == onlineMenuDialog.value) {
    if (dialog.returnValue == '')
      return show(mainMenuDialog.value!);

    if (dialog.returnValue == 'start') {
      connect({ kind: MessageKind.Start, passcode: passcode.value });
    } else if (dialog.returnValue == 'join') {
      location.hash = '#' + gameId.value;
    }
  } else if (dialog == joinDialog.value) {
    if (dialog.returnValue == 'join') {
      send({ kind: MessageKind.Start, passcode: passcode.value });
    }
  }

  openDialog.value = undefined;
}

function setGameId(id: string) {
  if (ws) {
    ws.onclose = ws.onmessage = null;
    ws.close();
    ws = undefined;
  }

  ourStone.value = undefined;

  if (id == '') {
    game.clear();
    return view.value!.draw();
  }

  if (id == 'local') {
    let encodedGame = localStorage.getItem('game');
    if (encodedGame) {
      game.assign(Game.deserialize(Base64.toUint8Array(encodedGame), true));
    } else {
      game.clear();
    }
    ourStone.value = game.turn();
    return view.value!.draw();
  }

  connect({ kind: MessageKind.Join, gameId: id });
}

function connect(initMsg: ClientMessage) {
  ws = new WebSocket('ws://' + document.location.hostname + ':8086/ws');
  ws.binaryType = "arraybuffer";
  ws.onopen = () => send(initMsg);
  ws.onclose = () => window.alert('Connection closed, please refresh the page.');
  ws.onmessage = onMessage;
}

function onMessage(e: MessageEvent) {
  let buf = new Uint8Array(e.data);
  let msg = ServerMessage.deserialize(buf);

  switch (msg.kind) {
    case MessageKind.Started:
      ourStone.value = msg.stone;
      if (msg.gameId)
        history.pushState(null, "", '#' + msg.gameId);
      break;
    case MessageKind.Game:
      game.assign(msg.game);
      view.value!.draw();
      if (!ourStone.value) show(joinDialog.value!);
      break;
    case MessageKind.Move:
      game.makeMove(msg.move);
      view.value!.draw();
      break;
    case MessageKind.Retract:
      game.undoMove();
      view.value!.draw();
      break;
  }
}

function onHashChange() {
  let dialog = openDialog.value;
  if (dialog) {
    openDialog.value = undefined;
    dialog.close();
  }

  setGameId(location.hash.slice(1));

  if (location.hash == '')
    return show(mainMenuDialog.value!);
}

onMounted(() => {
  onHashChange();
  window.addEventListener('hashchange', onHashChange);
});
</script>

<template>
  <GameView :game="game" :our-stone="ourStone" :disabled="openDialog != undefined" ref="view" @move="onMove"
    @undo="onUndo" @redo="onRedo" />

  <dialog ref="main-menu-dialog" @close="onDialogClose">
    <form method="dialog">
      <p><strong>Main Menu</strong></p>
      <div class="menu-btn-group">
        <button>Play Offline</button>
        <button value="online">Play Online</button>
      </div>
    </form>
  </dialog>

  <dialog ref="online-menu-dialog" @close="onDialogClose">
    <form method="dialog">
      <p><strong>Play Online</strong></p>
      <div class="radio-group">
        <label><input type="radio" name="action" value="start" v-model="onlineAction" checked />Start</label>
        <label><input type="radio" name="action" value="join" v-model="onlineAction" />Join</label>
      </div>
      <template v-if="onlineAction == 'start'">
        <label for="passcode">Passcode: </label>
        <input type="text" id="passcode" v-model="passcode" autocomplete="on" required size="12"
          placeholder="Yours, not shared" />
      </template>
      <template v-else>
        <label for="game-id">Game ID: </label>
        <input type="text" id="game-id" v-model="gameId" pattern="[0-9A-Za-z]{10}" autocomplete="on" required size="12"
          placeholder="10 number/letters" />
      </template>
      <div class="btn-group">
        <button formnovalidate>Cancel</button>
        <button v-if="onlineAction == 'start'" value="start">Start</button>
        <button v-else value="join">Join</button>
      </div>
    </form>
  </dialog>

  <dialog ref="join-dialog" @close="onDialogClose">
    <form method="dialog">
      <p><strong>Join Game</strong></p>
      <label for="passcode">Passcode: </label>
      <input type="text" id="passcode" v-model="passcode" autocomplete="on" required size="12"
        placeholder="Yours, not shared" />
      <div class="btn-group">
        <button formnovalidate>View Only</button>
        <button value="join">Join</button>
      </div>
    </form>
  </dialog>
</template>

<style>
body {
  /* Remove the default 8px margin from body. */
  margin: 0;
  background-color: #ffcc66;
}

/* Use `svh` to prevent overflow on mobile due to the hidable address bar. */
@supports (height: 100svh) {
  #app {
    height: 100svh;
  }
}

/* Old browsers might not support `svh`. */
@supports not (height: 100svh) {
  #app {
    height: 100vh;
  }
}

p {
  margin-top: 0;
  margin-bottom: 8px;
  text-align: center;
}

.menu-btn-group button {
  display: block;
  width: 100%;
}

.menu-btn-group button:not(:last-child) {
  margin-bottom: 5px;
}

.radio-group {
  margin-bottom: 5px;
  display: flex;
  justify-content: center;
}

.radio-group label:not(:last-child) {
  margin-right: 10px;
}

input[type="text"] {
  text-align: center;
}

.btn-group {
  margin-top: 10px;
  display: flex;
  justify-content: space-evenly;
}

.btn-group button {
  width: 100%;
}

.btn-group button:not(:last-child) {
  margin-right: 10px;
}
</style>