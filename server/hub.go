// Copyright 2013 The Gorilla WebSocket Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

package main

import (
	"c6ol/game"
	"encoding/json"
	"strconv"
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
			client.send <- h.boardToJson()
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

func (h *Hub) boardToJson() []byte {
	out := make([]int, h.board.Index())
	for i, move := range h.board.Record() {
		out[i] = move.Pos.Index()
	}
	outJson, err := json.Marshal(out)
	if err != nil {
		panic("failed to convert board to JSON")
	}
	return outJson
}

func (h *Hub) handleMessage(msg []byte) {
	n, err := strconv.ParseInt(string(msg), 10, 32)
	if err != nil {
		return
	}

	if n >= 0 {
		p := game.PointFromIndex(int(n))
		stone, _ := h.board.InferTurn()
		if !h.board.Set(p, stone) {
			return
		}
		if h.board.FindWinRow(p) != nil {
			h.board.Jump(0)
		}
	} else if h.board.Unset() == nil {
		return
	}

	boardJson := h.boardToJson()

	for client := range h.clients {
		select {
		case client.send <- boardJson:
		default:
			close(client.send)
			delete(h.clients, client)
		}
	}
}
