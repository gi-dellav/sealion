function plus(x, y) {
    return x + y;
}
const seq_seq = list(list(1, 2, 3), list(4, 5, 6), 
                     list(7, 8, 9), list(10, 11, 12));
accumulate_n(plus, 0, seq_seq);
