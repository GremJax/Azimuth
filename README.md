# Azimuth

The azimuth coding language is different from object oriented programming, where objects conform to a single type with known variables.
Instead, each object can conform to any number of classes, with its variables mapped from the other classes it conforms to.
This prioritizes dynamic capabilities and graph-like data structures while still allowing for type safety and speed.

## Structure

Instead of structs or classes, capabilities are defined through shapes. Shapes are like a template that can be applied upon an object.
Each shape has a number of default mappings, functions, and slots which act as variables. These are all referenced by an azimuth, a structure that contains information and a unique id. Multiple azimuths can point to the same values in memory based on the object's unique mapping. Shapes can be attached to an object with or without a mapping, and any non-mapped values are allocated from memory. Shapes can similarly be detached at runtime, which removes the mappings to those values, but the values will not be deallocated until all mappings are detached.

## Syntax

### Atlas

The atlas file is where your project is defined and organized. It contains the dependencies for your project and its exports.
In the root directory of your project, the compiler is provided a named atlas to be the root of your project.
Atlas syntax looks like this:

    dependencies {
        std -> lang/std/atlas.atls
        foo -> https://github.com/foo/atlas.atls
    }
    chart MyProgram {
        Main        -> main.az
    }

The "chart" section is where you define your modules. The identifier following the word "chart" is the name of your project.
The compiler will load all of the files according to their dependencies, and execute them one at a time in the specified load order.
Namespaces can be defined by file, by namespace definition within a file, or by entire directory.
Other atlases can be linked as sub-namespaces of the super one.

    chart MyProgram {
        Main        -> main.az              // File
        SomeUtil    -> utils.az::SomeUtil   // Interior Namespace
        Rendering   -> src/rendering        // Directory
        Physics     -> physics/physics.atls // Atlas
    }

All shapes and static azimuths within an atlas-defined namespace can be accessed with the "using" keyword within another namespace. All namespaces necessarily use themself and all parent namespaces as well. Namespaces within other namespaces are accessed with double colons.

    namespace SomeUtil {
        using std::Math

        print Sum(5, 10)
    }

All defined namespaces are public externally. The "hidden" keyword makes namespaces only public within their own atlas.

    chart MyProgram {
        Main    -> main.az
        hidden Secrets -> secrets.az
    }

The "trailhead" keyword defines an entry point to the program without running the entire thing. The namespace's identifier must be used intead of the program's when running from a trailhead. There is an argument handler in the std to help with this.

    chart MyProgram {
        trailhead API       -> api.az
        trailhead Structure -> structure.az
        hidden backend      -> backend.az
    }

### Statements

The azimuth language features similar syntax to Rust, Swift, and other modern languages, with several notable differences. All statements can be contained in blocks or not, and all expressions can be contained in parenthesis or not: it is up to the developer when clarity is more important than space. Semicolons are not mandatory, whitespace is not significant, and braces are only required where nested statements would become ambiguous.

Long chains of consecutive statements are valid:

    if x == y print "true" else print "false"

...and maximally verbose statements are valid:

    if (x == y) {
        print "true";
    } else {
        print "false";
    }

The recommended design is one that prioritizes simplicity above all else, with expressions contained in the minimum amount of parenthesis and statements on new lines following their conditions:

    if x == y
        print "true"
    else print "false"

Braces are required for multiple lines

    if x == y {
        print "true"
        print "still true"
        print "hello world"
    } else print "false"

For loops and while loops function how you would expect

    for i in 0 ... 10 {
        print i
        print x[i]
    }

    while x <= y {
        print x
        x += 1
    }

    loop {
        x += 1

        if x > y 
            break
    }

Array indexing is done with brackets.

    print x[i]
    x[i + 1] = 5

String formats are also easy as pie with breakout brackets: "\\{ }"

    print "Hello my name is \{person.name}!"

### Functions

Functions are written just like any other azimuth, but with parenthesis following the name with a number of parameters and their types. Then, an optional arrow with a return type, and the code to be executed.

    PrintFoo() 
        print "foo"
    
    PrintBar(string x) {
        print "bar"
        print x
    }

Object parameters can be listed with multiple input types for multiple minimum requirements.

    PrintIdAndName(Named Identified x) {
        print "Id: \{x.Id}, Name: \{x.Name}"
    }

Functions are non-static implicitly and can be made static with the "static" keyword. Adding "self" as the first parameter makes it into a static function acting upon the caller as "self", which is the exact explicit form of the first pattern.

    shape Foo {

        DoSomething() {
            print self
        }

        static DoSomething() {
            print "static"
        }

        static DoSomething(self) {
            print self
        }
    }

Functions can be given a return type. Functions must then return the appropriate type within the function statement.

    GetName() -> string
        return self.Name

    PrintNameAndReturn() -> string {
        print self.Name
        return self.GetName()
    }

Lymphnoids (aka Lambdas) are written with a group of input variables, an arrow, and a statement, totally surrounded by pipes:

    |x -> print x|      // Single param
    |(x, y) -> x + y|   // Multiple params
    |() -> print "foo"| // Action

    dictionary.Map(|(key, value) -> value := Marker(key)|)

Lymphnoid types are the same syntax, just with types instead of names

    |int|
    |(int, int) -> int|
    |() -> string|[]

Lymphnoids are actually just function signatures in the compiler, so any function works

    foo(|(int32, int32) -> int32| function, int32 x, int32 y) 
        print function(x, y)

    foo(|(x, y) -> x + y|, 5, 6)    // lymphnoids work

    sum(int32 x, int32 y) -> int32 
        return x + y

    foo(sum, 5, 6)                // named functions also work

### Operators

Every arithmatic operator is here!

    -x
    x + y
    x - y
    x * y
    x / y
    x % y
    x += y
    x -= y
    x *= y
    x /= y
    x %= y

    ~x
    x & y
    x | y
    x ^ y
    x << y
    x >> y
    x &= y
    x |= y
    x ^= y
    x <<= y
    x >>= y

Comparisons are also routine. For the boolean operators, the word or symbol may be used.

    x and y
    x && y
    x or y
    x || y
    !x

    x == y
    x != y
    x < y
    x > y
    x <= y
    x >= y

Ranges are defined with elipses, "..." for end inclusive and "..<" for end exclusive. This works in positive and negative directions.

    x ... y
    x ..< y

Ternaries are also supported

    print (x == y) ? someThing : someThingElse

Numbers can be written with various prefixes. Underscores separate digits visually.

    0xFF        // Hex
    0o778       // Octal
    0b1100_1111 // Binary
    0c123_456   // Decimal (default)

### Shapes

Shapes are defined anywhere within a file and contain a number of azimuth and function definitions.

Shapes are defined as follows:

    shape Foo {
    
        Bar int32

        Baz string

        Qux() print "hello world"
    
    }

Shapes can inherit from other shapes by listing their names after its own, following a colon:

    shape ChildOfFoo : Foo {

        PrintFoo() -> int32 {
            print "foo"
            return 0
        }

    }

Any object that is attached ChildOfFoo will also be attached Foo first, with all its values remapped according to the default provided mapping:

    shape ChildOfFoo : Foo(X -> Bar) {

        X int32 = 0

    }

You can also map functions for chaining and overrides

    shape ChildOfFoo : Foo(
        SuperPrint -> after Foo::Print
        Foo::Something -> OverriddenThing
    ) {
        OverriddenThing() -> {
            print "this is an override"
        }

        SuperPrint() -> {
            print "hello"
        }
    }

Azimuths can be locked, which makes it impossible to detach them even if the shape is detached. They can also be declared static, which applies them to the shape's static singleton and accessed through the shape name directly. They can also be declared const, which makes their value immutable

    shape Foo {

        static GlobalCount int32 = 0

        locked Count int32 = 0

        const MaxCount int32 = 100

    }

The "attach" and "detach" keywords are special functions called on an object when it is attached and detached. Constants are mutable in these functions.

    shape Foo {

        const MaxCount int32

        attach() {
            self.MaxCount = 100
        }

        detach() {
            print "detached"
        }
    }

These can be given additional parameters which are required to attach the shape

    shape Foo {

        const MaxCount int32

        attach(maxCount int32) {
            self.MaxCount = maxCount
        }

    }

    let obj := Foo(
        maxCount = 5
    )

Functions and azimuths can also be abstract, which must be remapped from the object when they are attached

    shape Foo {
        abstract Data int32[]
    }

    let obj := List<int32>
    obj := Foo(
        Data -> List::Array
    )

Azimuths can have optional types just like Swift

    shape Foo {
        Bar int32?
    }

    print object.Bar ?? 0

### Objects

Objects are declared using the "let" keyword. These override names in the scope and persist through references.

    let foo

Shapes can be attached to objects by using the ":=" attachment operator:

    let foo := SomeShape

    foo := SomeOtherShape

These can also be attached with mappings by using parenthesis and arrows for each azimuth mapping:

    foo := Vector2 (
        X -> Width
        Y -> Height
    )

Similarly shapes can be detached by using the "=:" detachment operator:

    foo =: SomeShape

To access the variables of an object, use a "." dot operator followed by the name of the azimuth that points to it:

    print foo.Name

There are times when there are identifiers with the same name on an object. This will cause a compiler error if there are multiple with the same name when it is called. In case of ambiguous names, the shape name can also be provided as a clarifier. This also allows lensing from specific inheritance layers.

    print foo::Named.Name

Multiple functions can be assigned to the same azimuth using the keywords "before" and "after" which will cause them to execute in a chain. This is how super/base overrides work, but also how you can attach additional functionality onto single functions.

    x := Printable
    x := Benchmarking (
        Benchmark -> before Printable::Print
    )

You can also "hijack" functions by attaching shapes to them directly with a reflection

    x := Printable
    x.Print := Benchmark (
        Start -> before Execute
        Stop -> after Execute
    )

To change the mapping, simply attach again. Unmodified mappings will be ignored, and the modified mappings will be moved.

    x := Vector2 (
        X -> Width
        Y -> Height
    )

    x := Vector2 (
        X -> Height
        Y -> Width
    )

Mappings can go either way from the corresponding shapes, and types are resolved by the base owner of the data.

    shape FloatThing {
        Value float32
    }

    shape IntThing {
        Value int32
    }

    object := FloatThing
    object := IntThing {
        Value -> FloatThing::Value  // Int in terms of float
    }

    print object.Value  // Value is a float

    object := FloatThing {
        Value -> IntThing::Value    // Float in terms of int
    }

    print object.Value  // Value is an int

Objects can be sealed using the "seal" keyword to prevent any additional attachments or detachments. This is important for safety or critical operations. Modified objects will need to be an unsealed copy. Attempting to attach or detach to the object will cause a runtime error.

    let user := UserData
    seal user

    user := DataStealer
    // Compile error

Objects can be tested for shape compatability using the "=~" is shape and "!~" isn't shape operators. These will compare whether an object meets the minimum mappings for a certain shape. This is strict to mappings based on the default mappings of the shape, but azimuths without default mappings could pull from anywhere.

    let x
    print x =~ Vector2
    // prints "false"

    x := Vector2
    print x =~ Vector2
    // prints "true"

### Reflection

Reflection has not been added to the language yet, but if it was, here is how it would be implemented. Remind me to add it when a genuine use is found, otherwise its just complications for no reason.

Reflection is accomplished using the "@" operator. There can be reflected instances of objects, shapes, and azimuths to provide information and mutate at the runtime object level. Such patterns can be used to attach shapes and azimuths to other shapes and azimuths directly, either on a single object or the static singleton of each class.

    @foo         // Reflected object
    foo.@Name    // Reflected Azimuth state
    Named::@Name // Reflected Azimuth (stateless)
    @Named       // Reflected Shape

    foo.@Name := Benchmarking {
        Start -> before Get
        Stop -> after Get
    }

Reflected types can be used with @Shape, @Azimuth, and @Object. These let you store reflected values and use them as parameters.

    PrintCompilerName(@Object obj)
        print obj.Slots

    GetReturnType(@Azimuth az) -> @Shape
        return az.ReturnType

    let typeStorage.Stored = @Identified
    
The "$" operator can turn reflected types and instances back into their object, shape, and azimuth counterparts. They can be stored as info and converted back through runtime lookups. Why you would ever want to do such a thing instead of just using generics and shape comparisons is beyond me, but its possible.

    CompareType(@Shape shape, Object object) -> bool
        return object =~ $shape

    ExecuteIfPresent(Action @Azimuth az, Object object) {
        if @object.Mappings.Contains(az.Id)
            object.$az()
    }
