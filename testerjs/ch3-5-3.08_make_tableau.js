function make_tableau(transform, s) {
    return pair(s, () => make_tableau(transform, transform(s)));
}
