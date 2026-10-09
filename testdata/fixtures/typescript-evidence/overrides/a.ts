export {};
class Animal { speak() {} walk() {} #secret() {} reveal() { this.#secret(); } }
class Dog extends Animal { override speak() {} #secret() {} }
function talk(a: Animal, d: Dog) { a.speak(); a.walk(); d.speak(); }
const make = () => Animal;
class Unknown extends (make()) { walk() {} }
function stroll(a: Animal) { a.walk(); }
