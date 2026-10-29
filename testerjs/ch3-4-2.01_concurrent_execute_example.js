// chapter=3 variant=concurrent 
let x = 10;

concurrent_execute(() => { x = x * x; },
                   () => { x = x + 1; });

// expected: 'all threads terminated'
