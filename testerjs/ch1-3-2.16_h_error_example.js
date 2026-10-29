const a = 1;
function f(x) {      
    const b = a + x;
    const a = 5;
    return a + b;
}
f(10);
