<script setup lang="ts">
import { computed, onMounted, reactive, ref, useTemplateRef, watch } from 'vue';
import GameView from './components/GameView.vue';
import { Move, MoveKind, Point, Record, Stone } from './game';
import { ClientMessage, MessageKind, ServerMessage } from './protocol';
import { decodeBase64, encodeBase64 } from '@std/encoding/base64';

const openDialogs = reactive(new Set<HTMLDialogElement>());

const mainMenuDialog = useTemplateRef('main-menu-dialog');
const onlineMenuDialog = useTemplateRef('online-menu-dialog');
const joinDialog = useTemplateRef('join-dialog');
const connClosedDialog = useTemplateRef('conn-closed-dialog');
const gameMenuDialog = useTemplateRef('game-menu-dialog');
const confirmDialog = useTemplateRef('confirm-dialog');
const errorDialog = useTemplateRef('error-dialog');

const altPressed = ref(false);

enum Action {
  MainMenu,
  Submit,
  Pass,
  OneStoneMove,
  OfferDraw,
  AcceptDraw,
  RequestRetract,
  AcceptRetract,
  Resign,
}

interface ConfirmRequest {
  message: string;
  confirm: string;
  cancel: string;
  confirmed(): void;
}

const onlineAction = ref('start');
const gameId = ref('');
const passcode = ref('');

const view = useTemplateRef('view');

const confirmRequest = ref<ConfirmRequest>();
const pendingConfirmRequests: ConfirmRequest[] = [];

const retractRequest = ref<Stone>();
const drawRequest = ref<Stone>();

const errorMessage = ref('');

function confirm(action: Action, confirmed: () => void) {
  let message, confirm = 'Confirm', cancel = 'Cancel';
  switch (action) {
    case Action.MainMenu:
      message = 'Back to main menu?';
      break;
    case Action.Submit:
      message = 'Submit the move?';
      break;
    case Action.Pass:
      message = 'Pass without placing stones?';
      break;
    case Action.OneStoneMove:
      message = 'Make a one-stone move?';
      break;
    case Action.OfferDraw:
      message = 'Offer a draw?';
      break;
    case Action.AcceptDraw:
      message = 'The opponent offers a draw.';
      confirm = 'Accept';
      cancel = 'Ignore';
      break;
    case Action.RequestRetract:
      message = 'Request to retract the previous move?';
      break;
    case Action.AcceptRetract:
      message = 'The opponent requests to retract the previous move.';
      confirm = 'Accept';
      cancel = 'Ignore';
      break;
    case Action.Resign:
      message = 'Resign the game?';
      break;
  }

  const req = { message, confirm, cancel, confirmed };
  if (confirmRequest.value) {
    pendingConfirmRequests.push(req);
  } else {
    confirmRequest.value = req;
    show(confirmDialog.value!);
  }
}

function confirmDraw() {
  const action = drawRequest.value ? Action.AcceptDraw : Action.OfferDraw;
  confirm(action, () => send({ kind: MessageKind.RequestDraw }));
}

function confirmRetract() {
  const action = retractRequest.value ? Action.AcceptRetract : Action.RequestRetract;
  confirm(action, () => send({ kind: MessageKind.RequestRetract }));
}

const record = reactive(new Record());
const ourStone = ref<Stone>();

watch(record, () => {
  if (gameId.value == 'local') save();
  if (gameId.value != '' && !ws)
    ourStone.value = record.turn();
});

let ws: WebSocket | undefined;

/** Sends the message on the WebSocket connection. */
function send(msg: ClientMessage) {
  if (ws && ws.readyState == WebSocket.OPEN)
    return ws.send(ClientMessage.encode(msg));

  errorMessage.value = 'WebSocket connection is not open.';
  show(errorDialog.value!);
}

/** Saves the record to local storage. */
function save() {
  const buf = encodeBase64(record.encode(true));
  localStorage.setItem('record', buf);
}

function onMenu() {
  show(gameMenuDialog.value!);
}

function onSubmit(pos: [Point] | [Point, Point]) {
  if (ws) {
    confirm(Action.Submit, () => send({ kind: MessageKind.Place, pos }));
  } else {
    record.makeMove({ kind: MoveKind.Stone, pos });
  }
}

function onPass() {
  const tentative = view.value!.tentative;
  if (ws) {
    const action = tentative ? Action.OneStoneMove : Action.Pass;

    let msg: ClientMessage;
    if (tentative) {
      msg = { kind: MessageKind.Place, pos: [tentative] };
    } else {
      msg = { kind: MessageKind.Pass };
    }

    confirm(action, () => send(msg));
  } else {
    let move: Move;
    if (tentative) {
      move = { kind: MoveKind.Stone, pos: [tentative] };
    } else {
      move = { kind: MoveKind.Pass };
    }

    record.makeMove(move);
  }
}

function onUndo() {
  if (!record.hasPast()) return;
  if (ws) {
    if (retractRequest.value != ourStone.value)
      confirmRetract();
  } else {
    record.undoMove();
  }
}

function onRedo() {
  if (!record.hasFuture()) return;
  if (!ws) record.redoMove();
}

function onHome() {
  if (!record.hasPast()) return;
  if (!ws) record.jump(0);
}

function onEnd() {
  if (!record.hasFuture()) return;
  if (!ws) record.jump(record.moves().length);
}

function show(dialog: HTMLDialogElement) {
  openDialogs.add(dialog);
  dialog.returnValue = '';
  dialog.showModal();
}

function onDialogClose(e: Event) {
  const dialog = e.target as HTMLDialogElement;
  const ret = dialog.returnValue;

  openDialogs.delete(dialog);
  if (ret == 'game-id-changed') return;

  if (dialog == mainMenuDialog.value) {
    if (ret == 'offline' || ret == '') {
      setGameId('local');
    } else if (ret == 'online') {
      show(onlineMenuDialog.value!);
    }
  } else if (dialog == onlineMenuDialog.value) {
    if (ret == 'start') {
      connect({ kind: MessageKind.Start, passcode: passcode.value });
    } else if (ret == 'join') {
      setGameId(gameId.value);
    } else if (ret == '') {
      show(mainMenuDialog.value!);
    }
  } else if (dialog == joinDialog.value) {
    if (ret == 'join') {
      send({ kind: MessageKind.Start, passcode: passcode.value });
    }
  } else if (dialog == connClosedDialog.value) {
    if (ret == 'retry') {
      setGameId(gameId.value);
    } else if (ret == '') {
      setGameId('');
    }
  } else if (dialog == gameMenuDialog.value) {
    altPressed.value = false;
    if (ret == 'main-menu') {
      if (ws) {
        confirm(Action.MainMenu, () => setGameId(''));
      } else {
        setGameId('');
      }
    } else if (ret == 'join') {
      show(joinDialog.value!);
    } else if (ret == 'pass') {
      onPass();
    } else if (ret == 'claim-win') {
      // TODO.
    } else if (ret == 'draw') {
      if (ws) {
        confirmDraw();
      } else {
        record.makeMove({ kind: MoveKind.Draw });
      }
    } else if (ret == 'resign') {
      if (ws) {
        confirm(Action.Resign, () => send({ kind: MessageKind.Resign }));
      } else {
        record.makeMove({ kind: MoveKind.Resign, stone: record.turn() });
      }
    } else if (ret == 'undo') {
      onUndo();
    } else if (ret == 'redo') {
      onRedo();
    } else if (ret == 'home') {
      onHome();
    } else if (ret == 'end') {
      onEnd();
    }
  } else if (dialog == confirmDialog.value) {
    if (ret == 'confirm') {
      confirmRequest.value!.confirmed();
    }
    if (confirmRequest.value = pendingConfirmRequests.shift()) {
      show(confirmDialog.value!);
    }
  } else if (dialog == errorDialog.value) {
    setGameId('');
  }
}

const ANALYZE_PREFIX = 'analyze,';

function setGameId(id: string) {
  if (ws) {
    ws.onclose = ws.onmessage = null;
    ws.close();
    ws = undefined;
  }

  for (const dialog of openDialogs)
    dialog.close('game-id-changed');

  if (id != location.hash.slice(1))
    history.pushState(null, '', '#' + id);

  gameId.value = id;
  ourStone.value = undefined;

  // Clear the requests.
  retractRequest.value = undefined;
  drawRequest.value = undefined;

  if (id == '') {
    record.clear();
    return show(mainMenuDialog.value!);
  }

  if (id == 'local') {
    const encodedRecord = localStorage.getItem('record');
    if (encodedRecord) {
      record.assign(Record.decode(decodeBase64(encodedRecord), 0, true));
    } else {
      record.clear();
    }
    return;
  }

  if (id.startsWith(ANALYZE_PREFIX)) {
    const encodedRecord = id.slice(ANALYZE_PREFIX.length);
    try {
      record.assign(Record.decode(decodeBase64(encodedRecord), 0, false));
    } catch (e) {
      console.error(e);
      errorMessage.value = 'Failed to decode record.';
      show(errorDialog.value!);
    }
    return;
  }

  if (!/^[0-9A-Za-z]{10}$/.test(id)) {
    errorMessage.value = 'Invalid game ID.';
    return show(errorDialog.value!);
  }

  connect({ kind: MessageKind.Join, gameId: id });
}

function connect(initMsg: ClientMessage) {
  ws = new WebSocket('ws://' + document.location.host + '/ws');
  ws.binaryType = 'arraybuffer';
  ws.onopen = () => send(initMsg);
  ws.onclose = e => onConnClose(e.code, e.reason);
  ws.onmessage = onMessage;
}

const CLOSE_CODE_ABNORMAL = 1006;
const CLOSE_CODE_POLICY = 1008;

const connClosedReason = ref('');

function onConnClose(code: number, reason: string) {
  if (reason == '') {
    if (code == CLOSE_CODE_ABNORMAL) {
      reason = 'Closed abnormally.';
    } else {
      reason = `Closed with code ${code}.`;
    }
  }
  connClosedReason.value = reason;
  show(connClosedDialog.value!);
}

function onMessage(e: MessageEvent) {
  let msg;
  try {
    msg = ServerMessage.decode(new Uint8Array(e.data));
  } catch (e) {
    console.error(e);
    ws!.close(CLOSE_CODE_POLICY, 'Malformed server message.');
    return;
  }

  switch (msg.kind) {
    case MessageKind.Started:
      ourStone.value = msg.stone;
      if (msg.gameId) {
        gameId.value = msg.gameId;
        history.pushState(null, '', '#' + msg.gameId);
        show(gameMenuDialog.value!);
      }
      if (drawRequest.value == Stone.opposite(msg.stone))
        confirmDraw();
      if (retractRequest.value == Stone.opposite(msg.stone))
        confirmRetract();
      return;
    case MessageKind.Record:
      record.assign(msg.record);
      if (!ourStone.value) show(joinDialog.value!);
      return;
    case MessageKind.Move:
      record.makeMove(msg.move);
      break;
    case MessageKind.Retract:
      record.undoMove();
      break;
    case MessageKind.RequestDraw:
      drawRequest.value = msg.stone;
      if (ourStone.value == Stone.opposite(msg.stone))
        confirmDraw();
      return;
    case MessageKind.RequestRetract:
      retractRequest.value = msg.stone;
      if (ourStone.value == Stone.opposite(msg.stone))
        confirmRetract();
      return;
  }

  // Clear the requests.
  drawRequest.value = undefined;
  retractRequest.value = undefined;
}

const gameStatus = computed(() => {
  if (!record.isEnded()) {
    return Stone[record.turn()] + ' to Play';
  }

  const prevMove = record.prevMove()!;
  if (prevMove.kind == MoveKind.Draw) {
    return 'Game Drawn';
  } else if (prevMove.kind == MoveKind.Resign) {
    return Stone[prevMove.stone] + ' Resigned';
  } else if (prevMove.kind == MoveKind.Win) {
    const stone = record.stoneAt(prevMove.pos)!;
    return Stone[stone] + ' Won';
  }
  return 'Bug';
});

function onHashChange() {
  setGameId(location.hash.slice(1));
}

onMounted(() => {
  onHashChange();
  window.addEventListener('hashchange', onHashChange);
});
</script>

<template>
  <!-- We need an explicit cast to work around a type mismatch. -->
  <GameView ref="view" :record="<Record>record" :our-stone="ourStone" :disabled="openDialogs.size != 0" @menu="onMenu"
    @submit="onSubmit" @undo="onUndo" @redo="onRedo" @home="onHome" @end="onEnd" />

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
        <input type="text" id="passcode" v-model="passcode" autocomplete="on" required
          placeholder="Yours, not shared" />
      </template>
      <template v-else>
        <label for="game-id">Game ID: </label>
        <input type="text" id="game-id" v-model="gameId" pattern="[0-9A-Za-z]{10}" autocomplete="on" required
          placeholder="10 alphanumerics" />
      </template>
      <div class="btn-group reversed">
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
      <input type="text" id="passcode" v-model="passcode" autocomplete="on" required placeholder="Yours, not shared" />
      <div class="btn-group reversed">
        <button value="join">Join</button>
        <button formnovalidate>View Only</button>
      </div>
    </form>
  </dialog>

  <dialog ref="conn-closed-dialog" @close="onDialogClose">
    <form method="dialog">
      <p><strong>Connection Closed</strong></p>
      <p>{{ connClosedReason }}</p>
      <div class="btn-group">
        <button>Menu</button>
        <button value="retry">Retry</button>
      </div>
    </form>
  </dialog>

  <dialog ref="game-menu-dialog" style="min-width: 11em;" @close="onDialogClose">
    <form method="dialog">
      <p><strong>Game Menu</strong></p>
      <p style="font-family: monospace;">
        <template v-if="gameId == 'local'">Offline</template>
        <template v-else-if="gameId.startsWith(ANALYZE_PREFIX)">Analyzing</template>
        <template v-else>
          <a :href="'#' + gameId">{{ gameId }}</a><br />
          {{ ourStone ? `Playing ${Stone[ourStone]}` : 'View Only' }}
        </template><br />
        {{ gameStatus }}<br />
        <a target="_blank" :href="'#' + ANALYZE_PREFIX + encodeBase64(record.encode(false))">Analyze</a>
      </p>
      <div class="menu-btn-group">
        <button value="main-menu">Main Menu</button>
        <button v-if="!ourStone" value="join">Join</button>
        <div v-if="ourStone" class="btn-group">
          <button @click.prevent="altPressed = !altPressed" :class="{ pressed: altPressed }">Alt</button>
          <button v-if="ws" value="undo" :disabled="!record.hasPast() || retractRequest == ourStone">Retract</button>
          <template v-else-if="!altPressed">
            <button value="undo" :disabled="!record.hasPast()">Undo</button>
            <button value="redo" :disabled="!record.hasFuture()">Redo</button>
          </template>
          <template v-else>
            <button value="home" :disabled="!record.hasPast()">Home</button>
            <button value="end" :disabled="!record.hasFuture()">End</button>
          </template>
        </div>
        <div v-if="ourStone" class="btn-group">
          <template v-if="!altPressed">
            <button value="claim-win" :disabled="record.isEnded()">Claim Win</button>
            <button value="resign" :disabled="record.isEnded()">Resign</button>
          </template>
          <template v-else>
            <button value="pass" :disabled="record.isEnded() || ourStone != record.turn()">Pass</button>
            <button value="draw" :disabled="record.isEnded() || drawRequest == ourStone">Draw</button>
          </template>
        </div>
        <button autofocus>Resume</button>
      </div>
    </form>
  </dialog>

  <dialog class="transparent" ref="confirm-dialog" @close="onDialogClose">
    <form method="dialog">
      <p>{{ confirmRequest?.message }}</p>
      <div class="btn-group">
        <button>{{ confirmRequest?.cancel }}</button>
        <button value="confirm">{{ confirmRequest?.confirm }}</button>
      </div>
    </form>
  </dialog>

  <dialog ref="error-dialog" @close="onDialogClose">
    <form method="dialog">
      <p><strong>Error</strong></p>
      <p>{{ errorMessage }}</p>
      <div class="btn-group">
        <button>Main Menu</button>
      </div>
    </form>
  </dialog>
</template>

<style>
body {
  /* Remove the default 8px margin from body. */
  margin: 0;
  background-color: #ffcc66;
  font-family: sans-serif;
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

dialog {
  max-width: 15em;
}

p {
  margin-top: 0;
  margin-bottom: 8px;
  text-align: center;
}

a {
  color: blue;
}

input[type="text"] {
  text-align: center;
  /* More consistent than the `size` attribute. */
  width: 8em;
}

button {
  width: 100%;
  user-select: none;
  white-space: nowrap;
}

.pressed {
  border-style: inset;
}

.menu-btn-group {
  display: flex;
  flex-direction: column;
}

.menu-btn-group>.btn-group {
  margin-top: 0;
}

.menu-btn-group>*:not(:last-child) {
  margin-bottom: 5px;
}

.radio-group {
  margin-bottom: 5px;
  display: flex;
  justify-content: center;
}

.radio-group>*:not(:last-child) {
  margin-right: 10px;
}

.btn-group {
  margin-top: 10px;
  display: flex;
  justify-content: space-evenly;
}

.btn-group:not(.reversed)>*:not(:last-child) {
  margin-right: 10px;
}

.btn-group.reversed {
  /* Show the default button (first in tree order) on the right. */
  flex-direction: row-reverse;
}

.btn-group.reversed>*:not(:last-child) {
  margin-left: 10px;
}

.transparent {
  opacity: 75%;
}
</style>
