import { math_floor, math_random } from './sicp.js';
function random(n) {
    return math_floor(math_random() * n);
}
