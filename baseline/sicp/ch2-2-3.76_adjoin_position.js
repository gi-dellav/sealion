import { pair } from './sicp.js';
function adjoin_position(row, col, rest) {
    return pair(pair(row, col), rest);
}
