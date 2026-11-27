import { list, map } from './sicp.js';
function list_of_unassigned(symbols) {
    return map(symbol => "*unassigned*", symbols);
}
