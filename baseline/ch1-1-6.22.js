function square(x) {
    return x * x;
}
function f(x, y, z) {
   return square(x) + square(y) + square(z) -
          // subtract the square of the smallest
          square(x > y ? (y > z ? z : y) : (x > z ? z : x));
}

f(4, 7, 2);
