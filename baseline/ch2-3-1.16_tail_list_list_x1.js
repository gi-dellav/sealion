import { list, tail } from './sicp.js';
tail(list(list("x1", "x2"), list("y1", "y2")));
// [["y1", ["y2", null]], null]

// expected: [ [ 'y1', [ 'y2', null ] ], null ]
