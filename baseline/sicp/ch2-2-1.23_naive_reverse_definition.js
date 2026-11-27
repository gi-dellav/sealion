import { append, head, is_null, list, pair, tail } from './sicp.js';
function reverse(items) {
    return is_null(items)
           ? null
           : append(reverse(tail(items)),
	            pair(head(items), null));
}

head(reverse(list(1, 4, 5, 9, 16, 25)));

// expected: 25
