put(list("a"), 1);
put(list("b", "c"), 2);
put(list("d", "e", "f"), 3);

display(get(list("a")));
display(get(list("b", "c")));
display(get(list("d", "e", "f")));

put(list("a", "b"), 1);
display(get(list("a")));
put(list("b", "c", "d"), 2);
display(get(list("b", "c")));
put(list("b"), 1);
display(get(list("b")));
