import { heart, stack, beside, show } from 'rune';

function split(bigger_op, smaller_op) {
    function rec_split(painter, n) {
        if (n === 0) {
            return painter;
        } else {
            const smaller = rec_split(painter, n - 1);
            return bigger_op(painter, 
                        smaller_op(smaller, smaller));
        }
    }
    return rec_split;
}
const right_split = split(beside, stack);

show(right_split(heart, 4));
