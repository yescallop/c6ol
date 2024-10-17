<script setup lang="ts">
import { onMounted, ref, useTemplateRef } from 'vue';
import GameView from './components/GameView.vue';
import { Move, MoveKind, Point, Record } from './game';
import { ClientMessage, MessageKind, ServerMessage } from './protocol';
import { decodeBase64, encodeBase64 } from '@std/encoding/base64';

const openDialog = ref<HTMLDialogElement>();

const mainMenuDialog = useTemplateRef('main-menu-dialog');
const onlineMenuDialog = useTemplateRef('online-menu-dialog');
const joinDialog = useTemplateRef('join-dialog');
const closedDialog = useTemplateRef('closed-dialog');

const onlineAction = ref('start');
const gameId = ref('');
const passcode = ref('');

const rec = new Record();
const view = useTemplateRef('view');

let ws: WebSocket | undefined;

/** Sends the message on the WebSocket connection. */
function send(msg: ClientMessage) {
  if (!ws || ws.readyState != WebSocket.OPEN)
    return onClose('Failed to send a message.');
  ws.send(ClientMessage.serialize(msg));
}

/** Saves the record to local storage. */
function save() {
  const buf = encodeBase64(rec.serialize(true));
  localStorage.setItem('record', buf);
}

function onMove(pos: [] | [Point] | [Point, Point]) {
  if (ws) {
    let msg: ClientMessage;
    if (pos.length == 0) {
      msg = { kind: MessageKind.Pass };
    } else {
      msg = { kind: MessageKind.Place, pos };
    }

    send(msg);
  } else {
    let move: Move;
    if (pos.length == 0) {
      move = { kind: MoveKind.Pass };
    } else {
      move = { kind: MoveKind.Stone, pos };
    }

    rec.makeMove(move);
    save();

    view.value!.stone = rec.turn();
    view.value!.draw();
  }
}

function onUndo() {
  if (ws) {
    send({ kind: MessageKind.RequestRetract });
  } else {
    rec.undoMove();
    save();

    view.value!.stone = rec.turn();
    view.value!.draw();
  }
}

function onRedo() {
  if (!ws) {
    rec.redoMove();
    save();

    view.value!.stone = rec.turn();
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

  const dialog = openDialog.value;

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
  } else if (dialog == closedDialog.value) {
    if (dialog.returnValue == 'refresh') {
      location.reload();
    } else if (dialog.returnValue == 'menu') {
      return show(mainMenuDialog.value!);
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

  view.value!.stone = undefined;

  if (id == '') {
    rec.clear();
    return view.value!.draw();
  }

  if (id == 'local') {
    const encodedRec = localStorage.getItem('record');
    if (encodedRec) {
      rec.assign(Record.deserialize(decodeBase64(encodedRec), 0, true));
    } else {
      rec.clear();
    }
    view.value!.stone = rec.turn();
    return view.value!.draw();
  }

  connect({ kind: MessageKind.Join, gameId: id });
}

function connect(initMsg: ClientMessage) {
  ws = new WebSocket('ws://' + document.location.host + '/ws');
  ws.binaryType = 'arraybuffer';
  ws.onopen = () => send(initMsg);
  ws.onclose = e => onClose(e.reason);
  ws.onmessage = onMessage;
}

const CLOSE_CODE_POLICY = 1008;

const closedReason = ref('');

function onClose(reason: string) {
  closedReason.value = reason;
  show(closedDialog.value!);
}

function onMessage(e: MessageEvent) {
  let msg;
  try {
    msg = ServerMessage.deserialize(new Uint8Array(e.data));
  } catch (e) {
    console.error(e);
    ws!.close(CLOSE_CODE_POLICY, 'malformed message');
    return;
  }

  switch (msg.kind) {
    case MessageKind.Started:
      view.value!.stone = msg.stone;
      view.value!.draw();
      if (msg.gameId)
        history.pushState(null, '', '#' + msg.gameId);
      break;
    case MessageKind.Record:
      rec.assign(msg.rec);
      view.value!.draw();
      if (!view.value!.stone) show(joinDialog.value!);
      break;
    case MessageKind.Move:
      rec.makeMove(msg.move);
      view.value!.draw();
      break;
    case MessageKind.Retract:
      rec.undoMove();
      view.value!.draw();
      break;
    case MessageKind.RequestDraw:
      // TODO.
      break;
    case MessageKind.RequestRetract:
      // TODO.
      break;
  }
}

function onHashChange() {
  const dialog = openDialog.value;
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
  <GameView :rec="rec" :disabled="openDialog != undefined" ref="view" @move="onMove" @undo="onUndo" @redo="onRedo" />

  <dialog ref="main-menu-dialog" @close="onDialogClose">
    <form method="dialog">
      <p><strong>Main Menu</strong></p>
      <div class="menu-btn-group">
        <button value="offline">Play Offline</button>
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
        <button v-if="onlineAction == 'start'" value="start">Start</button>
        <button v-else value="join">Join</button>
        <button formnovalidate>Cancel</button>
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
        <button value="join">Join</button>
        <button formnovalidate>View Only</button>
      </div>
    </form>
  </dialog>

  <dialog ref="closed-dialog" @close="onDialogClose">
    <form method="dialog">
      <p><strong>Disconnected</strong></p>
      <p>{{ closedReason }}</p>
      <div class="btn-group">
        <button value="refresh">Refresh</button>
        <button value="menu">Menu</button>
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

.btn-group button:last-child {
  margin-right: 10px;
}

.btn-group button:first-child {
  order: 1;
}
</style>
