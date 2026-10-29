function plus(x, y) {
    return x + y;
}
function accumulate_n(op, init, seqs) {
    return is_null(head(seqs))
           ? null
           : pair(accumulate(op, init, map(x => head(x), seqs)),
                  accumulate_n(op, init, map(x => tail(x), seqs)));
}

const seq_seq = list(list(1, 2, 3), list(4, 5, 6), 
                     list(7, 8, 9), list(10, 11, 12));
accumulate_n(plus, 0, seq_seq);

// expected: [ 22, [ 26, [ 30, null ] ] ]
