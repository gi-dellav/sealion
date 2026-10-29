// chapter=3 variant=non-det 
function a_pythagorean_triple_between(low, high) {
    const i = an_integer_between(low, high);
    const hsq = high * high;
    const j = an_integer_between(i, high);
    const ksq = i * i + j * j;
    require(hsq >= ksq);
    const k = math_sqrt(ksq);
    require(is_integer(k));
    return list(i, j, k);
}

function is_integer(x) {
    return x === math_floor(x);
}
a_pythagorean_triple_between(5, 15);
