import { append, head, is_null, tail } from './sicp.js';
// encode_symbol function to be written by students
function encode(message, tree) {
    return is_null(message)
           ? null
           : append(encode_symbol(head(message), tree),
                    encode(tail(message), tree));
}
