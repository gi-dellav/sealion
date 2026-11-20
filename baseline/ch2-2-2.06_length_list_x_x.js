import { length, list, pair } from './sicp.js';
const x = pair(list(1, 2), list(3, 4));
length(list(x, x));

// expected: 2
