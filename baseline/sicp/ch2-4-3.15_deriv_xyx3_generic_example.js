import { head, list, tail } from './sicp.js';
head(tail(head(tail(deriv(list("*", list("*", "x", "y"), list("+", "x", 3)), "x")))));
