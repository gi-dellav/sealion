import { heart, stack, beside, show } from 'rune';

function up_split(painter, n) {
    if (n === 0) {
        return painter;
    } else {
        const smaller = up_split(painter, n - 1);
        return stack(beside(smaller, smaller), painter);
    }
}

show(up_split(heart, 4));
