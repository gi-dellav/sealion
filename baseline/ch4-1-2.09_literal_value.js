import { head, parse, tail } from './sicp.js';
function literal_value(component) {    
    return head(tail(component));
}

literal_value(parse("null;"));

// expected: null
