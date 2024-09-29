package game

// A 2D point with integer coordinates.
type Point struct {
	X int
	Y int
}

// Returns the absolute value of `n`.
func abs(n int) int {
	if n < 0 {
		return -n
	}
	return n
}

// Returns the index of the point on the spiral.
func (p Point) Index() int {
	if p.X == 0 && p.Y == 0 {
		return 0
	}

	xAbs, yAbs := abs(p.X), abs(p.Y)
	vertical := xAbs > yAbs
	var k int
	if vertical {
		k = xAbs
	} else {
		k = yAbs
	}
	t := k << 1
	// m=(t-1)^2

	if vertical {
		if p.X > 0 {
			// Right: m+y+k-1
			t -= 1
			return t*t + p.Y + k - 1
		} else {
			// Left: m+2t+k-1-y
			return t*t + k - p.Y
		}
	} else if p.Y > 0 {
		// Top: m+t-1+k-x
		return t*t - t + k - p.X
	} else {
		// Bottom: m+3t-1+x+k
		return t*t + t + p.X + k
	}
}

// Computes the integer square root of `s`.
func isqrt(s uint) uint {
	if s <= 1 {
		return s
	}
	x0 := s >> 1
	x1 := (x0 + s/x0) >> 1
	for x1 < x0 {
		x0 = x1
		x1 = (x0 + s/x0) >> 1
	}
	return x0
}

// Creates a point from its index on the spiral.
func PointFromIndex(n int) Point {
	if n == 0 {
		return Point{0, 0}
	}

	k := int((isqrt(uint(n)) + 1) >> 1)
	t := k << 1
	m := t*t + 1 // m=(t-1)^2+2t

	if n < m {
		m -= t
		if n < m {
			// Right
			return Point{k, k + 1 - (m - n)}
		} else {
			// Top
			return Point{k - 1 - (n - m), k}
		}
	} else {
		m += t
		if n < m {
			// Left
			return Point{-k, -k - 1 + (m - n)}
		} else {
			// Bottom
			return Point{-k + 1 + (n - m), -k}
		}
	}
}

// Returns the adjacent point in the direction of the axis.
func (p Point) Adjacent(axis Axis, forward bool) Point {
	dx, dy := axis.UnitVec()
	if forward {
		return Point{p.X + dx, p.Y + dy}
	} else {
		return Point{p.X - dx, p.Y - dy}
	}
}

// A contiguous row of stones on the board.
type Row struct {
	start Point
	end   Point
}

// A stone on the board.
type Stone int

const (
	NoStone = Stone(iota)
	BlackStone
	WhiteStone
)

// Returns the opposite stone.
func (s Stone) Opposite() Stone {
	switch s {
	case BlackStone:
		return WhiteStone
	case WhiteStone:
		return BlackStone
	}
	return NoStone
}

// An axis on the board.
type Axis int

const (
	VerticalAxis = Axis(iota)
	AscendingAxis
	HorizontalAxis
	DescendingAxis
)

var Axes = []Axis{VerticalAxis, AscendingAxis, HorizontalAxis, DescendingAxis}

// Returns the unit vector in the direction of the axis.
func (a Axis) UnitVec() (int, int) {
	switch a {
	case VerticalAxis:
		return 0, 1
	case AscendingAxis:
		return 1, -1
	case HorizontalAxis:
		return 1, 0
	case DescendingAxis:
		return 1, 1
	}
	return 0, 0
}

// A move on the board, namely a (position, stone) pair.
type Move struct {
	Pos   Point
	Stone Stone
}

// An infinite Connect6 board.
type Board struct {
	board map[Point]Stone
	moves []Move
	index int
}

// Creates an empty board.
func NewBoard() Board {
	return Board{make(map[Point]Stone), make([]Move, 0), 0}
}

// Returns the total number of moves, on or off the board,
// in the past or in the future.
func (b *Board) Total() int {
	return len(b.moves)
}

// Returns the current move index.
func (b *Board) Index() int {
	return b.index
}

// Tests if the board is empty.
func (b *Board) Empty() bool {
	return b.index == 0
}

// Returns the stone at a point.
func (b *Board) Get(p Point) Stone {
	return b.board[p]
}

// Returns a slice of moves in the past.
func (b *Board) PastMoves() []Move {
	return b.moves[:b.index]
}

// Makes a move at a point, clearing moves in the future.
func (b *Board) Set(p Point, stone Stone) bool {
	if stone == NoStone {
		panic("setting no stone")
	}
	if _, ok := b.board[p]; ok {
		return false
	}
	b.board[p] = stone

	b.moves = b.moves[:b.index]
	b.moves = append(b.moves, Move{p, stone})
	b.index++
	return true
}

// Undoes the last move (if any).
func (b *Board) Unset() *Move {
	if b.index == 0 {
		return nil
	}
	b.index--
	last := b.moves[b.index]

	delete(b.board, last.Pos)
	return &last
}

// Redoes the next move (if any).
func (b *Board) Reset() *Move {
	if b.index >= len(b.moves) {
		return nil
	}
	next := b.moves[b.index]
	b.index++

	b.board[next.Pos] = next.Stone
	return &next
}

// Jumps to the given move index by undoing or redoing moves.
func (b *Board) Jump(index int) {
	if index > len(b.moves) {
		return
	}
	if b.index < index {
		for i := b.index; i < index; i++ {
			next := b.moves[i]
			b.board[next.Pos] = next.Stone
		}
	} else {
		for i := b.index - 1; i >= index; i-- {
			last := b.moves[i]
			delete(b.board, last.Pos)
		}
	}
	b.index = index
}

// Infers the next stone to play and whether the opponent
// is to play after that, based on past moves.
func (b *Board) InferTurn() (Stone, bool) {
	if b.index == 0 {
		return BlackStone, true
	}

	last := b.moves[b.index-1].Stone
	if b.index == 1 {
		return WhiteStone, last == WhiteStone
	}

	prevOfLast := b.moves[b.index-2].Stone
	if last == prevOfLast {
		return last.Opposite(), false
	}
	return last, true
}

// Scans the row through a point in the direction of the axis.
func (b *Board) ScanRow(p Point, axis Axis) (row Row, len int) {
	stone := b.Get(p)
	if stone == NoStone {
		return
	}
	len = 1

	scan := func(cur *Point, forward bool) {
		next := cur.Adjacent(axis, forward)
		for b.Get(next) == stone {
			len += 1
			*cur = next
			next = cur.Adjacent(axis, forward)
		}
	}

	row = Row{p, p}
	scan(&row.start, false)
	scan(&row.end, true)
	return
}

// Searches for a win row through the point.
func (b *Board) FindWinRow(p Point) *Row {
	stone := b.Get(p)
	if stone == NoStone {
		return nil
	}

	for _, axis := range Axes {
		row, len := b.ScanRow(p, axis)
		if len >= 6 {
			return &row
		}
	}
	return nil
}
