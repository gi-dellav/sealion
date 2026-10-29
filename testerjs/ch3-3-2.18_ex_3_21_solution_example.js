const q1 = make_queue();
print_queue(q1); // prints: null
insert_queue(q1, "a");
print_queue(q1); // prints: ["a", null]
insert_queue(q1, "b");
print_queue(q1); // prints: ["a", ["b", null]]
delete_queue(q1);
print_queue(q1); // prints: ["b", null]
