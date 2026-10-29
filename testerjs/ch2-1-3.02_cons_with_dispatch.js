function pair(x, y) {
    function dispatch(m) {
        return m === 0 
               ? x
               : m === 1 
               ? y
               : error(m, "argument not 0 or 1 -- pair");
    }
    return dispatch;	      
}
function head(z) { return z(0); }

function tail(z) { return z(1); }

const x = pair(1, 2);
head(x);

// expected: 1
