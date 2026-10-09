// What the TypeScript checker says each call, construction, and named argument targets.
//
// The questions follow the proof of concept's checker evidence (graph-compiler, commit 5c98a6e,
// compilers/codegraph/frontend/typescript/evidence.ts): the callee's symbol, aliases followed,
// union receivers expanded per constituent, augmentations set aside. The declarations the checker
// names are then mapped to the one concrete body they denote (docs/graph-model.md, "Value policy
// and reason order", contexts `call`, `construct`, and `reference`) and onto Unfold's blocks by
// exact range. Policy refusals that do not change which body the checker names (an override in a
// subclass, an unbound `this`) are recorded beside the target, because the proof of concept's
// recall denominator left them out.

import ts from "typescript";

/** Transparent wrappers: parentheses, non-null assertions, and type assertions. */
export function unwrap(node) {
  let current = node;
  while (
    ts.isParenthesizedExpression(current) || ts.isNonNullExpression(current) || ts.isAsExpression(current)
    || ts.isSatisfiesExpression(current) || ts.isTypeAssertionExpression(current)
  ) current = current.expression;
  return current;
}

/** Like `unwrap`, but keeps type assertions: they are the receiver's written type. */
function unwrapKeepingAssertions(node) {
  let current = node;
  while (ts.isParenthesizedExpression(current) || ts.isNonNullExpression(current) || ts.isSatisfiesExpression(current)) {
    current = current.expression;
  }
  return current;
}

function unalias(checker, symbol) {
  let current = symbol;
  const seen = new Set();
  while (current && current.flags & ts.SymbolFlags.Alias) {
    if (seen.has(current)) return undefined;
    seen.add(current);
    const next = checker.getAliasedSymbol(current);
    if (next === current) return undefined;
    current = next;
  }
  return current;
}

function accessRoot(expression) {
  let node = unwrap(expression);
  while (ts.isPropertyAccessExpression(node) || ts.isElementAccessExpression(node)) node = unwrap(node.expression);
  return node;
}

function containingClass(node) {
  for (let ancestor = node.parent; ancestor; ancestor = ancestor.parent) {
    if (ts.isClassLike(ancestor)) return ancestor;
  }
  return undefined;
}

/** `this` in a non-arrow function that is not a class member has no receiver to name. */
function thisIsUnbound(thisNode) {
  for (let ancestor = thisNode.parent; ancestor; ancestor = ancestor.parent) {
    if (ts.isClassLike(ancestor)) return false;
    if (ts.isFunctionLike(ancestor) && !ts.isArrowFunction(ancestor)) {
      const member = ts.isMethodDeclaration(ancestor) || ts.isGetAccessorDeclaration(ancestor)
        || ts.isSetAccessorDeclaration(ancestor) || ts.isConstructorDeclaration(ancestor);
      return !(member && ts.isClassLike(ancestor.parent));
    }
  }
  return true;
}

const hasModifier = (node, kind) => ts.canHaveModifiers(node) && (ts.getModifiers(node) ?? []).some(modifier => modifier.kind === kind);
const isAmbient = node => hasModifier(node, ts.SyntaxKind.DeclareKeyword) || !!(node.flags & ts.NodeFlags.Ambient);
const isAugmentation = node => ts.isModuleDeclaration(node) || ts.isInterfaceDeclaration(node) || ts.isTypeAliasDeclaration(node);
const isLiteralKey = node => ts.isStringLiteral(node) || ts.isNumericLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node);
const isFunctionValue = node => ts.isFunctionExpression(node) || ts.isArrowFunction(node);

/**
 * A property of an object literal that is a constant's initializer (`const o = { f: () => {} }`).
 * The proof of concept's checker policy never treated a property assignment as a body; C-2 made
 * this one shape concrete. Properties of other literals (returned, nested, or passed) stay values.
 */
function constantLiteralMember(property) {
  const literal = property.parent;
  let holder = literal.parent;
  while (holder && (ts.isParenthesizedExpression(holder) || ts.isAsExpression(holder) || ts.isSatisfiesExpression(holder)
    || ts.isNonNullExpression(holder) || ts.isTypeAssertionExpression(holder))) holder = holder.parent;
  return !!holder && ts.isVariableDeclaration(holder) && !!holder.initializer && unwrap(holder.initializer) === literal
    && !!(ts.getCombinedNodeFlags(holder) & ts.NodeFlags.Constant);
}

/** The written name of a member, or `undefined` for a computed one. */
function memberName(node) {
  if (ts.isConstructorDeclaration(node)) return "constructor";
  const name = node.name;
  return name && (ts.isIdentifier(name) || ts.isPrivateIdentifier(name) || isLiteralKey(name)) ? name.text : undefined;
}

/**
 * Evaluates call sites and arguments with one program's checker.
 *
 * `locate(node)` returns `{ path, start, end }` (UTF-8 bytes, getStart to end) for a node in an
 * indexed file, or `undefined` outside the graph; `blockKey(path, start, end, kind)` returns the
 * Unfold block with exactly that range and kind.
 */
export class CheckerQuestions {
  constructor(checker, locate, blockKey) {
    this.checker = checker;
    this.locate = locate;
    this.blockKey = blockKey;
  }

  /** Identity of a class declaration in the graph, for the override census. */
  classIdentity(node) {
    const place = this.locate(node);
    return place && `${place.path}#${place.start}:${place.end}`;
  }

  /** The class a receiver's type denotes: one class across a union's non-null constituents. */
  classForType(type) {
    if (type.isUnion()) {
      const classes = new Set(type.types
        .filter(part => !(part.flags & (ts.TypeFlags.Null | ts.TypeFlags.Undefined)))
        .map(part => this.classForType(part)));
      return classes.size === 1 ? [...classes][0] : undefined;
    }
    return type.getSymbol()?.declarations?.find(declaration => ts.isClassLike(declaration));
  }

  /** The block a declaration denotes in `context`, or why it denotes none. */
  concrete(declaration, context) {
    const place = this.locate(declaration);
    if (!place) return { external: true };
    let node = declaration;
    if (ts.isExportAssignment(node)) {
      const value = unwrap(node.expression);
      if (!(isFunctionValue(value) || ts.isClassExpression(value))) return { reason: "not_concrete" };
      node = value;
    }
    if (ts.isBindingElement(node)) {
      let root = node.parent;
      while (root && (ts.isBindingElement(root) || ts.isObjectBindingPattern(root) || ts.isArrayBindingPattern(root))) root = root.parent;
      if (root && ts.isParameter(root)) return { reason: "parameter" };
      if (root && ts.isVariableDeclaration(root) && !(ts.getCombinedNodeFlags(root) & ts.NodeFlags.Constant)) return { reason: "mutable_binding" };
      return { reason: "not_concrete" };
    }
    if (ts.isParameter(node)) return { reason: "parameter" };
    if (ts.isGetAccessorDeclaration(node) || ts.isSetAccessorDeclaration(node)) return { reason: "accessor" };
    if (ts.isVariableDeclaration(node)) {
      if (isAmbient(node)) return { reason: "not_concrete" };
      if (!(ts.getCombinedNodeFlags(node) & ts.NodeFlags.Constant)) return { reason: "mutable_binding" };
      const value = node.initializer && unwrap(node.initializer);
      if (!value || !(isFunctionValue(value) || ts.isClassExpression(value))) return { reason: "not_concrete" };
      node = value;
    } else if (ts.isPropertyAssignment(node) || ts.isPropertyDeclaration(node)) {
      if (isAmbient(node) || (ts.isPropertyAssignment(node) && !constantLiteralMember(node))) return { reason: "not_concrete" };
      // Only a constant's class expression is a class body (as in the proof of concept's policy).
      const value = node.initializer && unwrap(node.initializer);
      if (!value || !isFunctionValue(value)) return { reason: "not_concrete" };
      node = value;
    }
    if (ts.isClassLike(node)) {
      if (isAmbient(node) || context === "call") return { reason: "not_concrete" };
      if (context === "construct") {
        const constructor = node.members.find(member => ts.isConstructorDeclaration(member) && member.body);
        if (constructor) return this.block(constructor, "constructor");
      }
      return this.block(node, "class");
    }
    if (ts.isFunctionDeclaration(node) || isFunctionValue(node)) {
      return node.body && !isAmbient(node) ? this.block(node, "function") : { reason: "not_concrete", signature: true };
    }
    if (ts.isMethodDeclaration(node)) {
      return node.body && !isAmbient(node) ? this.block(node, "method") : { reason: "not_concrete", signature: true };
    }
    return { reason: "not_concrete" };
  }

  block(node, kind) {
    const place = this.locate(node);
    const key = place && this.blockKey(place.path, place.start, place.end, kind);
    return key ? { key, kind } : { unmapped: `${kind} at ${place?.path}:${place?.start}-${place?.end}` };
  }

  /** A body-less signature that is an overload of `implementation`. */
  static isOverload(signature, implementation) {
    const kinds = [ts.SyntaxKind.FunctionDeclaration, ts.SyntaxKind.MethodDeclaration, ts.SyntaxKind.Constructor];
    return kinds.includes(signature.kind) && !signature.body && !isAmbient(signature)
      && signature.kind === implementation.kind && signature.parent === implementation.parent
      && memberName(signature) === memberName(implementation);
  }

  /** One concrete target from the declarations the checker names, or the outcome without one. */
  decide(declarations, context) {
    if (!declarations.length) return { outcome: "unknown" };
    const effective = declarations.filter(declaration => !isAugmentation(declaration));
    if (!effective.length) return { outcome: declarations.every(declaration => !this.locate(declaration)) ? "external" : "not_concrete" };
    const answers = effective.map(declaration => ({ declaration, answer: this.concrete(declaration, context) }));
    const indexed = answers.filter(item => !item.answer.external);
    if (!indexed.length) return { outcome: "external" };
    if (indexed.length !== answers.length) return { outcome: "ambiguous" };
    const targets = [...new Set(indexed.filter(item => item.answer.key).map(item => item.answer.key))];
    if (targets.length > 1) return { outcome: "ambiguous" };
    if (!targets.length) {
      const unmapped = indexed.find(item => item.answer.unmapped);
      if (unmapped) return { outcome: "unmapped", detail: unmapped.answer.unmapped };
      for (const reason of ["parameter", "mutable_binding", "accessor"]) {
        if (indexed.some(item => item.answer.reason === reason)) return { outcome: reason };
      }
      return { outcome: "not_concrete" };
    }
    const chosen = indexed.find(item => item.answer.key === targets[0]);
    const strays = indexed.filter(item => !item.answer.key && !CheckerQuestions.isOverload(item.declaration, chosen.declaration));
    if (strays.length) return { outcome: "ambiguous" };
    if (context === "reference" && !["function", "method", "class"].includes(chosen.answer.kind)) return { outcome: "unsupported" };
    return { target: targets[0], declaration: chosen.declaration };
  }

  /** Declarations named by a callee or value expression, with what the policy needs about it. */
  declarationsOf(expression) {
    const checker = this.checker;
    const value = unwrap(expression);
    if (isFunctionValue(value) || ts.isClassExpression(value)) return { declarations: [value] };
    if (value.kind === ts.SyntaxKind.SuperKeyword) {
      const owner = containingClass(value);
      const base = owner?.heritageClauses?.find(clause => clause.token === ts.SyntaxKind.ExtendsKeyword)?.types[0]?.expression;
      const symbol = base && unalias(checker, checker.getSymbolAtLocation(base));
      return { declarations: symbol?.declarations ?? [], superAccess: true };
    }
    const access = ts.isPropertyAccessExpression(value) || ts.isElementAccessExpression(value) ? value : undefined;
    const literal = access && ts.isElementAccessExpression(access) && isLiteralKey(unwrap(access.argumentExpression))
      ? unwrap(access.argumentExpression).text : undefined;
    const nameNode = access ? (ts.isPropertyAccessExpression(access) ? access.name : access.argumentExpression) : value;
    let symbol = checker.getSymbolAtLocation(nameNode);
    if (!symbol && literal !== undefined) symbol = checker.getPropertyOfType(checker.getTypeAtLocation(access.expression), literal);
    let declarations = unalias(checker, symbol)?.declarations ?? [];
    const facts = { superAccess: false, staticAccess: false, receiverClass: undefined, unboundThis: false };
    if (access) {
      const receiver = unwrap(access.expression);
      const root = accessRoot(receiver);
      facts.unboundThis = root.kind === ts.SyntaxKind.ThisKeyword && thisIsUnbound(root);
      facts.superAccess = receiver.kind === ts.SyntaxKind.SuperKeyword;
      const receiverType = checker.getTypeAtLocation(access.expression);
      const typed = unwrapKeepingAssertions(access.expression);
      facts.receiverClass = typed.kind === ts.SyntaxKind.ThisKeyword ? containingClass(typed) : this.classForType(receiverType);
      const name = ts.isPropertyAccessExpression(access) ? access.name.text : literal;
      if (name !== undefined && receiverType.isUnion()) {
        declarations = receiverType.types
          .filter(type => !(type.flags & (ts.TypeFlags.Undefined | ts.TypeFlags.Null)))
          .flatMap(type => unalias(checker, checker.getPropertyOfType(type, name))?.declarations ?? []);
      }
      facts.staticAccess = declarations.some(declaration => hasModifier(declaration, ts.SyntaxKind.StaticKeyword));
    }
    return { declarations, ...facts };
  }

  /**
   * The answer for one callee or value: `{ target, refused?, override? }` or `{ outcome }`.
   * `override` carries what the override census needs; it is decided once every class is known.
   */
  answer(expression, context) {
    const facts = this.declarationsOf(expression);
    const decision = this.decide(facts.declarations, context);
    if (!decision.target) return decision;
    const result = { target: decision.target };
    if (facts.unboundThis) result.refused = "unbound_this";
    const member = decision.declaration;
    const owner = member.parent && ts.isClassLike(member.parent) ? member.parent : undefined;
    const checked = (ts.isMethodDeclaration(member) || ts.isPropertyDeclaration(member)) && owner;
    if (checked && !facts.superAccess && !facts.staticAccess && !hasModifier(member, ts.SyntaxKind.StaticKeyword)
      && !(member.name && ts.isPrivateIdentifier(member.name))) {
      const receiver = (facts.receiverClass && this.classIdentity(facts.receiverClass)) ?? this.classIdentity(owner);
      const ownerId = this.classIdentity(owner);
      if (receiver && ownerId) result.override = { receiver, owner: ownerId, name: memberName(member) ?? null };
    }
    return result;
  }

  /** The census shape of one class: its declared base and the members that can override. */
  classShape(node) {
    const checker = this.checker;
    const identity = this.classIdentity(node);
    if (!identity) return undefined;
    const baseExpression = node.heritageClauses?.find(clause => clause.token === ts.SyntaxKind.ExtendsKeyword)?.types[0]?.expression;
    let base;
    if (baseExpression) {
      const symbol = unalias(checker, checker.getSymbolAtLocation(baseExpression));
      for (const declaration of symbol?.declarations ?? []) {
        if (ts.isClassLike(declaration)) base = declaration;
        else if (ts.isVariableDeclaration(declaration) && declaration.initializer
          && ts.getCombinedNodeFlags(declaration) & ts.NodeFlags.Constant && ts.isClassExpression(unwrap(declaration.initializer))) {
          base = unwrap(declaration.initializer);
        }
      }
      if (!base) {
        const bases = new Set(checker.getTypeAtLocation(baseExpression).getConstructSignatures()
          .map(signature => this.classForType(signature.getReturnType())));
        if (bases.size === 1) base = [...bases][0];
      }
    }
    const members = [];
    for (const member of node.members) {
      const hasBody = (ts.isMethodDeclaration(member) || ts.isConstructorDeclaration(member) || ts.isAccessor(member)) ? !!member.body
        : ts.isPropertyDeclaration(member) && !!member.initializer && isFunctionValue(unwrap(member.initializer));
      members.push({
        name: memberName(member) ?? null,
        isStatic: hasModifier(member, ts.SyntaxKind.StaticKeyword),
        canOverride: !ts.isConstructorDeclaration(member) && !isAmbient(member) && (hasBody || ts.isPropertyDeclaration(member)),
      });
      if (ts.isConstructorDeclaration(member)) {
        for (const parameter of member.parameters) {
          if (ts.isParameterPropertyDeclaration(parameter, member) && ts.isIdentifier(parameter.name)) {
            members.push({ name: parameter.name.text, isStatic: false, canOverride: true });
          }
        }
      }
    }
    const baseId = base && this.classIdentity(base);
    return { identity, bases: baseId ? [baseId] : [], members };
  }
}

/** Decides every recorded override question once all class shapes are known. */
export function overrideCensus(shapes) {
  const byId = new Map(shapes.map(shape => [shape.identity, shape]));
  const descendants = new Map();
  for (const shape of shapes) {
    const pending = [...shape.bases];
    const seen = new Set();
    while (pending.length) {
      const base = pending.pop();
      if (seen.has(base)) continue;
      seen.add(base);
      if (!descendants.has(base)) descendants.set(base, []);
      descendants.get(base).push(shape);
      pending.push(...(byId.get(base)?.bases ?? []));
    }
  }
  return ({ receiver, name }) => (descendants.get(receiver) ?? []).some(shape => shape.identity !== receiver
    && shape.members.some(member => !member.isStatic && member.canOverride && (name === null || member.name === null || member.name === name)));
}
