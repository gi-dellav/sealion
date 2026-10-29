function adjoin_position(row, col, rest) {
    return pair(pair(row, col), rest);
}
const empty_board = null;
function is_safe(k, positions) {
    const first_row = head(head(positions));
    const first_col = tail(head(positions));
    return accumulate((pos, so_far) => {
                         const row = head(pos);
                         const col = tail(pos);
                         return so_far &&
                                first_row - first_col !==
                                row - col &&
                                first_row + first_col !==
                                row + col &&
                                first_row !== row;
                      },
                      true, 
                      tail(positions));
}
function flatmap(f, seq) {
    return accumulate(append, null, map(f, seq));
}
function enumerate_interval(low, high) {
    return low > high
           ? null
           : pair(low,
                  enumerate_interval(low + 1, high));
}
function queens(board_size) {
    function queen_cols(k) {
        return k === 0
               ? list(empty_board)
               : filter(positions => is_safe(k, positions),
                        flatmap(rest_of_queens =>
                                  map(new_row =>
                                        adjoin_position(new_row, k,
                                                        rest_of_queens),
                                      enumerate_interval(1, board_size)),
                                queen_cols(k - 1)));
    }
    return queen_cols(board_size);
}
length(queens(8));


// expected: 92
