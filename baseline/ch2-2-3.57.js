import { accumulate, append, length, list, map, pair } from './sicp.js';
function enumerate_interval(low, high) {
    return low > high
           ? null
           : pair(low,
                  enumerate_interval(low + 1, high));
}
const n = 6;
length(accumulate(append, 
           null, 
           map(i => map(j => list(i, j),
                        enumerate_interval(1, i - 1)),
               enumerate_interval(1, n))));

// expected: 15
