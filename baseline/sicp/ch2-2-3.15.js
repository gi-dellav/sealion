import { head, is_null, list, pair, tail } from './sicp.js';
function accumulate(op, initial, sequence) {
    return is_null(sequence)
           ? initial
           : op(head(sequence), 
                accumulate(op, initial, tail(sequence)));
}
tail(tail(accumulate(pair, null, list(1, 2, 3, 4, 5))));

// expected: [ 3, [ 4, [ 5, null ] ] ]
