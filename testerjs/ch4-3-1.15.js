// solution by GitHub user jonathantorres

function an_integer_between(low, high) {
    require(low <= high);
    return amb(low, an_integer_between(low+1, high));
}
