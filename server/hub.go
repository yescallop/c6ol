// Copyright 2013 The Gorilla WebSocket Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

package main

import (
	"c6ol/game"
	"encoding/binary"
)

// Hub maintains the set of active clients and broadcasts messages to the
// clients.
type Hub struct {
	// Registered clients.
	clients map[*Client]bool

	// Inbound messages from the clients.
	broadcast chan []byte

	// Register requests from the clients.
	register chan *Client

	// Unregister requests from clients.
	unregister chan *Client

	// The shared Connect6 board.
	board game.Board
}

func newHub() *Hub {
	return &Hub{
		broadcast:  make(chan []byte),
		register:   make(chan *Client),
		unregister: make(chan *Client),
		clients:    make(map[*Client]bool),
		board:      game.NewBoard(),
	}
}

func (h *Hub) run() {
	for {
		select {
		case client := <-h.register:
			h.clients[client] = true
			// Send the board when a client joins.
			client.send <- h.board.Serialize()
		case client := <-h.unregister:
			if _, ok := h.clients[client]; ok {
				delete(h.clients, client)
				close(client.send)
			}
		case msg := <-h.broadcast:
			h.handleMessage(msg)
		}
	}
}

// Handles messages from clients.
func (h *Hub) handleMessage(msg []byte) {
	if len(msg) == 0 {
		// Retract the last move (if any). For testing only.
		if h.board.Unset() == nil {
			return
		}
	} else {
		x, read := binary.Uvarint(msg)
		if read != len(msg) || x > 0xffffffff {
			// Data remaining or varint out of range.
			return
		}

		p := game.PointFromIndex(uint32(x))
		stone, _ := h.board.InferTurn()
		if !h.board.Set(p, stone) {
			// Fail if there is already a stone at the position.
			return
		}
		if h.board.FindWinRow(p) != nil {
			// Clear the board if we detect a win. For testing only.
			h.board.Jump(0)
		}
	}

	response := h.board.Serialize()

	// Broadcast the new board.
	// TODO: Use incremental updates.
	for client := range h.clients {
		select {
		case client.send <- response:
		default:
			close(client.send)
			delete(h.clients, client)
		}
	}
}
