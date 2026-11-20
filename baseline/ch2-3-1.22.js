import { equal, list } from './sicp.js';
// function equal to be written by student
equal(list("this", "is", "a", "list"), list("this", list("is", "a"), "list"));

// expected: false
