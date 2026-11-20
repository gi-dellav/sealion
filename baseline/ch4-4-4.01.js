import { head, is_null, pair, tail } from './sicp.js';
function append(x, y) {    
    return is_null(x)
           ? y
           : pair(head(x), append(tail(x), y));
}
