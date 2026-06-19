;; Keywords
"module" @keyword
"structure" @keyword
"functor" @keyword
"fn" @keyword
"let" @keyword
"type" @keyword
"effect" @keyword
"op" @keyword
"macro" @keyword
"match" @keyword
"if" @keyword
"else" @keyword
"with" @keyword
"handle" @keyword
"perform" @keyword
"unsafe" @keyword
"return" @keyword
"effects" @keyword
"capabilities" @keyword
"justification" @keyword
"block" @keyword
"linear" @keyword
"affine" @keyword
"true" @keyword
"false" @keyword

;; Comments
(comment) @comment

;; String and number literals
(string) @string
(number) @number

;; Unit literal
(unit) @constant

;; Boolean literals
(boolean) @constant

;; Identifiers and variable references
(identifier) @variable

;; Functions and macros
(function_decl name: (identifier) @function)
(macro_decl name: (identifier) @function.macro)
(single_rule_macro name: (identifier) @function.macro)
(multi_rule_macro name: (identifier) @function.macro)

;; Module names
(module_decl name: (identifier) @module)

;; Type names
(type_decl name: (identifier) @type)
(effect_decl name: (identifier) @type)

;; Function parameters
(parameter (identifier) @variable.parameter)

;; Effect row
(effect_row (identifier) @constant)

;; Capability row
(capability_row (identifier) @constant)

;; Operators
"=" @operator
"->" @operator
"+" @operator
"-" @operator
"*" @operator
"/" @operator
"!" @operator
"(" @punctuation.bracket
")" @punctuation.bracket
"{" @punctuation.bracket
"}" @punctuation.bracket
"[" @punctuation.bracket
"]" @punctuation.bracket
"," @punctuation.delimiter
":" @punctuation.delimiter
";" @punctuation.delimiter
"." @punctuation.delimiter
"|" @punctuation.delimiter

;; Type annotations
(parameter ":" @punctuation.delimiter)
(let_decl ":" @punctuation.delimiter)
(type_decl "=" @operator)
(function_decl "->" @operator)
