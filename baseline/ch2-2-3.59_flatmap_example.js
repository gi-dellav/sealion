import { length, list, map } from './sicp.js';
length(flatmap(x => list(x, x), list(1, 2, 3, 4)));
