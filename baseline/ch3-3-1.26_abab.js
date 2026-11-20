import { list } from './sicp.js';
list(list("a", "b"), "a", "b");

// expected: [ [ 'a', [ 'b', null ] ], [ 'a', [ 'b', null ] ] ]
