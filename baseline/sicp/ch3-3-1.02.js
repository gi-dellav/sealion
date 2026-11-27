import { list, pair, tail } from './sicp.js';
const x = list(list("a", "b"), "c");
const y = list("e", "f");
const z = pair(y, tail(x));
