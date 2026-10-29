// chapter=3 variant=concurrent 
let x = 10;

concurrent_execute(() => { x = x * x; }, 
                   () => { x = x * x * x; });

// expected: 'all threads terminated'
