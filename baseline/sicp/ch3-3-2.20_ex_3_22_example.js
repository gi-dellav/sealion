const q = make_queue();
print_queue(q); // prints: null
insert_queue(q, "a");
print_queue(q); // prints: ["a", null]
insert_queue(q, "b");
print_queue(q); // prints: ["a", ["b", null]]
delete_queue(q);
print_queue(q); // prints: ["b", null]
