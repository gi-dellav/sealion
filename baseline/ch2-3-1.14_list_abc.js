import { list } from './sicp.js';
list("a", "b", "c");
// ["a", ["b", ["c", null]]]

// expected: [ 'a', [ 'b', [ 'c', null ] ] ]
