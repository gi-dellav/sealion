import { head, length, list, pair, tail } from './sicp.js';
const x = pair(list(1, 2), list(3, 4));
length(head(tail(list(x, x))));

// expected: 3
