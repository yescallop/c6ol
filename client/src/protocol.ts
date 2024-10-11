import { Game, Move, Stone } from './c6';

export enum MessageKind {
  // Client messages.
  Start,
  Join,

  // Server messages.
  Started,
  Game,

  // Common messages.
  Move,
  Retract,
}

export type ClientMessage = {
  // When sent upon connection, starts a new game.
  // When sent after `Join`, authenticates the user.
  kind: MessageKind.Start;
  passcode: string;
} | {
  // When sent upon connection, adds the user to an existing game.
  kind: MessageKind.Join;
  gameId: string;
} | {
  // Requests a move.
  kind: MessageKind.Move;
  move: Move;
} | {
  // Requests to retract the last move.
  kind: MessageKind.Retract;
};

export namespace ClientMessage {
  export async function serialize(msg: ClientMessage): Promise<Uint8Array> {
    let buf;
    switch (msg.kind) {
      case MessageKind.Start:
        let pass = new TextEncoder().encode(msg.passcode);
        let hash = await crypto.subtle.digest('SHA-256', pass);
        buf = Uint8Array.of(MessageKind.Start, ...new Uint8Array(hash));
        break;
      case MessageKind.Join:
        let id = new TextEncoder().encode(msg.gameId);
        buf = Uint8Array.of(MessageKind.Join, ...id);
        break;
      case MessageKind.Move:
        buf = Uint8Array.of(MessageKind.Move, ...Move.serialize(msg.move, false));
        break;
      case MessageKind.Retract:
        buf = Uint8Array.of(msg.kind);
        break;
    }
    return buf;
  }
}

export type ServerMessage = {
  // Successfully authenticated.
  // Sent before `Game` if a new game is started.
  kind: MessageKind.Started;
  /** The user's stone. */
  stone: Stone;
  /** The game ID if a new game is started. */
  gameId?: string;
} | {
  // The entire game is updated.
  kind: MessageKind.Game;
  game: Game;
} | {
  // A move was made.
  kind: MessageKind.Move;
  move: Move;
} | {
  // The last move was retracted.
  kind: MessageKind.Retract;
};

export namespace ServerMessage {
  export function deserialize(buf: Uint8Array): ServerMessage {
    if (buf.length == 0) throw new RangeError('empty message');

    let kind = buf[0];
    buf = buf.subarray(1);

    let msg, stone;
    switch (kind) {
      case MessageKind.Started:
        if (buf.length == 0) throw new RangeError('empty payload');
        stone = Stone.fromNumber(buf[0]);

        let gameId;
        if (buf.length > 1)
          gameId = new TextDecoder().decode(buf.subarray(1));

        msg = { kind, stone, gameId };
        break;
      case MessageKind.Game:
        msg = { kind, game: Game.deserialize(buf) };
        break;
      case MessageKind.Move:
        let [move, n] = Move.deserialize(buf, false);
        if (n != buf.length)
          throw new RangeError('extra data');
        msg = { kind, move };
        break;
      case MessageKind.Retract:
        msg = { kind };
        break;
      default:
        throw new RangeError('unknown message kind');
    }
    return msg;
  }
}
