// chapter=3 variant=concurrent 
function test_and_set(cell) {
    if (head(cell)) {
        return true;
    } else {
        set_head(cell, true);
        return false;
    }
}

// expected: 'all threads terminated'
