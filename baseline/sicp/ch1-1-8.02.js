import { math_exp, math_log } from './sicp.js';
function square(x) {
    return math_exp(double(math_log(x)));
}
function double(x) {
    return x + x;
}

square(14);
