import { pair, tail } from './sicp.js';
function enumerate_interval(low, high) {
    return low > high
           ? null
           : pair(low,
                  enumerate_interval(low + 1, high));
}
tail(tail(tail(enumerate_interval(2, 7))));
