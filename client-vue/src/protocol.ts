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
  Request = 10,
}

/** A player's request. */
export enum Request {
  /** Ends the game in a draw. */
  Draw = 0,
  /** Retracts the previous move. */
  Retract = 1,
  /** Resets the game. */
  Reset = 2,
}

export namespace Request {
  /** All requests available. */
  export const VALUES = [Request.Draw, Request.Retract, Request.Reset];

  /** Creates a request from a number. */
  export function fromNumber(n: number): Request {
    if (n != 0 && n != 1 && n != 2)
      throw new RangeError('unknown request kind');
    return n;
  }
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
  // Makes a request.
  kind: MessageKind.Request;
  request: Request;
};

export namespace ClientMessage {
  export function encode(msg: ClientMessage) {
    const buf = [Uint8Array.of(msg.kind)];
    switch (msg.kind) {
      case MessageKind.Start:
        buf.push(new TextEncoder().encode(msg.passcode));
        break;
      case MessageKind.Join:
        buf.push(new TextEncoder().encode(msg.gameId));
        break;
      case MessageKind.Place:
        for (const p of msg.pos) p.encode(buf);
        break;
      case MessageKind.ClaimWin:
        msg.pos.encode(buf);
        break;
      case MessageKind.Request:
        buf.push(Uint8Array.of(msg.request));
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
  record: Record;
} | {
  // A move was made.
  kind: MessageKind.Move;
  move: Move;
} | {
  // The previous move was retracted.
  kind: MessageKind.Retract;
} | {
  // A player made a request.
  kind: MessageKind.Request;
  request: Request;
  stone: Stone;
};

export namespace ServerMessage {
  export function decode(buf: Uint8Array): ServerMessage {
    let i = 0;
    if (i >= buf.length) throw new RangeError('empty message');

    const kind = buf[i++];
    let msg, stone;
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
        msg = { kind, record: Record.decode(buf, i, false) };
        i = buf.length;
        break;
      case MessageKind.Move:
        let move;
        [move, i] = Move.decode(buf, i, false);
        msg = { kind, move };
        break;
      case MessageKind.Retract:
        msg = { kind };
        break;
      case MessageKind.Request:
        if (i + 2 > buf.length)
          throw new RangeError('expected request kind and stone');
        msg = {
          kind,
          request: Request.fromNumber(buf[i]),
          stone: Stone.fromNumber(buf[i + 1]),
        };
        i += 2;
        break;
      default:
        throw new RangeError('unknown message kind');
    }

    if (i < buf.length) throw new RangeError('extra data');
    return msg;
  }
}
