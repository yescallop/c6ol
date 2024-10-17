import { Move, Point, Record, Stone } from './game';
import { concat } from '@std/bytes/concat';

/// The kind of a WebSocket message.
export enum MessageKind {
  // Client messages.
  Start = 0,
  Join = 1,
  Place = 2,
  Pass = 3,
  ClaimWin = 4,
  Resign = 5,
  // Server messages.
  Started = 6,
  Record = 7,
  Move = 8,
  Retract = 9,
  // Common messages.
  RequestDraw = 10,
  RequestRetract = 11,
}

export type ClientMessage = {
  // When sent upon connection, requests to start a new game.
  // When sent after `Join`, requests to authenticate.
  kind: MessageKind.Start;
  passcode: string;
} | {
  // When sent upon connection, requests to join an existing game.
  kind: MessageKind.Join;
  gameId: string;
} | {
  // Requests to place one or two stones.
  kind: MessageKind.Place;
  pos: [Point] | [Point, Point];
} | {
  // Requests to pass.
  kind: MessageKind.Pass;
} | {
  // Claims a win.
  kind: MessageKind.ClaimWin;
  pos: Point;
} | {
  // Resigns the game.
  kind: MessageKind.Resign;
} | {
  // Requests a draw.
  kind: MessageKind.RequestDraw;
} | {
  // Requests to retract the previous move.
  kind: MessageKind.RequestRetract;
};

export namespace ClientMessage {
  export function serialize(msg: ClientMessage) {
    let buf = [Uint8Array.of(msg.kind)];
    switch (msg.kind) {
      case MessageKind.Start:
        buf.push(new TextEncoder().encode(msg.passcode));
        break;
      case MessageKind.Join:
        buf.push(new TextEncoder().encode(msg.gameId));
        break;
      case MessageKind.Place:
        for (let p of msg.pos) p.serialize(buf);
        break;
      case MessageKind.ClaimWin:
        msg.pos.serialize(buf);
        break;
    }
    return concat(buf);
  }
}

export type ServerMessage = {
  // The user is authenticated.
  // Sent before `Record` if a new game is started.
  kind: MessageKind.Started;
  /** The user's stone. */
  stone: Stone;
  /** The game ID if a new game is started. */
  gameId?: string;
} | {
  // The entire record is updated.
  kind: MessageKind.Record;
  rec: Record;
} | {
  // A move was made.
  kind: MessageKind.Move;
  move: Move;
} | {
  // The previous move was retracted.
  kind: MessageKind.Retract;
} | {
  // A player requested a draw.
  kind: MessageKind.RequestDraw;
  stone: Stone;
} | {
  // A player requested to retract the previous move.
  kind: MessageKind.RequestRetract;
  stone: Stone;
};

export namespace ServerMessage {
  export function deserialize(buf: Uint8Array): ServerMessage {
    let i = 0;
    if (i >= buf.length) throw new RangeError('empty message');

    let kind = buf[i++], msg, stone;
    switch (kind) {
      case MessageKind.Started:
        if (i >= buf.length) throw new RangeError('empty payload');
        stone = Stone.fromNumber(buf[i++]);

        let gameId;
        if (i < buf.length) {
          gameId = new TextDecoder().decode(buf.subarray(i));
          i = buf.length;
        }

        msg = { kind, stone, gameId };
        break;
      case MessageKind.Record:
        msg = { kind, rec: Record.deserialize(buf, i, false) };
        i = buf.length;
        break;
      case MessageKind.Move:
        let move;
        [move, i] = Move.deserialize(buf, i, false);
        msg = { kind, move };
        break;
      case MessageKind.Retract:
        msg = { kind };
        break;
      case MessageKind.RequestDraw:
      case MessageKind.RequestRetract:
        if (i >= buf.length) throw new RangeError('empty payload');
        msg = { kind, stone: Stone.fromNumber(buf[i++]) };
        break;
      default:
        throw new RangeError('unknown message kind');
    }

    if (i < buf.length) throw new RangeError('extra data');
    return msg;
  }
}
