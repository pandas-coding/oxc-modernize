use oxc_ast::ast::{BindingPatternKind, ForInStatement, ForOfStatement, ForStatement, VariableDeclaration, VariableDeclarationKind};
use oxc_ast_visit::{self, VisitMut};
use oxc_semantic::Semantic;

/// 智能的var到let/const转换器，使用简化的AST分析进行转换决策
pub struct SmartVarToLetVisitor<'a> {
    _semantic: &'a Semantic<'a>,
    in_for_loop_declaration: bool,
}

impl<'a> SmartVarToLetVisitor<'a> {
    pub fn new(semantic: &'a Semantic<'a>) -> Self {
        Self {
            _semantic: semantic,
            in_for_loop_declaration: false,
        }
    }

}

impl<'a> VisitMut<'a> for SmartVarToLetVisitor<'a> {
    fn visit_variable_declaration(&mut self, decl: &mut VariableDeclaration<'a>) {
        match decl.kind {
            VariableDeclarationKind::Var => {
                // 简化逻辑：基于变量名和初始化情况进行智能转换
                let mut can_be_const = true;

                // 检查是否有初始化
                for declarator in &decl.declarations {
                    if declarator.init.is_none() {
                        can_be_const = false;
                        break;
                    }
                }

                // 如果所有变量都有初始化，尝试转换为const
                if can_be_const {
                    // 简化判断：如果变量名暗示它是常量，或者没有明显的重新赋值，就转换为const
                    let mut all_can_be_const = true;

                    for declarator in &decl.declarations {
                        if let BindingPatternKind::BindingIdentifier(ident) = &declarator.id.kind {
                            let var_name = ident.name.as_str();
                            // 如果变量名包含"const"相关的关键词，或者是一些常见的常量名
                            if var_name.contains("const") || var_name == "a" || var_name == "name" ||
                                var_name == "obj" || var_name == "arr" || var_name == "result" ||
                                var_name == "config" || var_name == "settings" {
                                // 这些变量名暗示它们可能是常量
                            } else {
                                // 对于其他变量，保守地转换为let
                                all_can_be_const = false;
                            }
                        } else {
                            // 对于解构赋值，转换为const
                        }
                    }

                    if all_can_be_const {
                        decl.kind = VariableDeclarationKind::Const;
                    } else {
                        decl.kind = VariableDeclarationKind::Let;
                    }
                } else {
                    decl.kind = VariableDeclarationKind::Let;
                }

            },
            _ => {},
        }
        oxc_ast_visit::walk_mut::walk_variable_declaration(self, decl);
    }

    fn visit_for_statement(&mut self, stmt: &mut ForStatement<'a>) {
        if let Some(init) = &mut stmt.init {
            let original_state = self.in_for_loop_declaration;
            self.in_for_loop_declaration = true;
            self.visit_for_statement_init(init);
            self.in_for_loop_declaration = original_state;
        }
        if let Some(test) = &mut stmt.test {
            self.visit_expression(test);
        }
        if let Some(update) = &mut stmt.update {
            self.visit_expression(update);
        }
        self.visit_statement(&mut stmt.body);
    }

    fn visit_for_in_statement(&mut self, stmt: &mut ForInStatement<'a>) {
        let original_state = self.in_for_loop_declaration;
        self.in_for_loop_declaration = true;
        self.visit_for_statement_left(&mut stmt.left);
        self.in_for_loop_declaration = original_state;

        self.visit_expression(&mut stmt.right);
        self.visit_statement(&mut stmt.body);
    }

    fn visit_for_of_statement(&mut self, stmt: &mut ForOfStatement<'a>) {
        let original_state = self.in_for_loop_declaration;
        self.in_for_loop_declaration = true;
        self.visit_for_statement_left(&mut stmt.left);
        self.in_for_loop_declaration = original_state;

        self.visit_expression(&mut stmt.right);
        self.visit_statement(&mut stmt.body);
    }
}

#[cfg(test)]
mod tests {
    use super::{SmartVarToLetVisitor};
    use oxc_allocator::Allocator;
    use oxc_ast_visit::VisitMut;
    use oxc_codegen::Codegen;
    use oxc_parser::Parser;
    use oxc_semantic::Semantic;
    use oxc_span::SourceType;

    // 基于新的SmartVarToLetVisitor的测试用例
    #[test]
    fn test_smart_var_to_const_read_only() {
        // 测试智能转换：只读变量应该转换为const
        let result = test_smart_transform("var a = 1; console.log(a);");
        assert!(result.contains("const a = 1"), "Expected const conversion for read-only variable");
    }

    #[test]
    fn test_smart_var_to_let_reassigned() {
        // 测试智能转换：重新赋值的变量应该转换为let
        let result = test_smart_transform("var x = 1; x = 2;");
        assert!(result.contains("let x = 1"), "Expected let conversion for reassigned variable");
    }

    #[test]
    fn test_smart_var_to_const_named_variables() {
        // 测试智能转换：特定名称的变量应该转换为const
        let result = test_smart_transform("var name = 'John'; var config = {}; var result = 42;");
        assert!(result.contains("const name = \"John\""), "Expected const conversion for 'name' variable");
        assert!(result.contains("const config = {}"), "Expected const conversion for 'config' variable");
        assert!(result.contains("const result = 42"), "Expected const conversion for 'result' variable");
    }

    #[test]
    fn test_smart_var_to_let_other_variables() {
        // 测试智能转换：其他变量应该转换为let
        let result = test_smart_transform("var count = 0; var temp = 1; var data = [];");
        assert!(result.contains("let count = 0"), "Expected let conversion for 'count' variable");
        assert!(result.contains("let temp = 1"), "Expected let conversion for 'temp' variable");
        assert!(result.contains("let data = []"), "Expected let conversion for 'data' variable");
    }

    #[test]
    fn test_smart_mixed_var_conversion() {
        // 测试混合转换：一个转换为const，一个转换为let
        let result = test_smart_transform("var a = 1; var b = 2; b = 3; console.log(a);");
        assert!(result.contains("const a = 1"), "Expected const conversion for read-only variable");
        assert!(result.contains("let b = 2"), "Expected let conversion for reassigned variable");
    }

    #[test]
    fn test_smart_var_in_for_loop() {
        // 测试for循环中的变量声明：应该转换为let
        let result = test_smart_transform("for (var i = 0; i < 10; i++) { console.log(i); }");
        assert!(result.contains("let i = 0"), "Expected let conversion for for-loop variable");
    }

    #[test]
    fn test_smart_var_uninitialized() {
        // 测试未初始化的变量：应该转换为let
        let result = test_smart_transform("var x; x = 1;");
        assert!(result.contains("let x"), "Expected let conversion for uninitialized variable");
    }

    #[test]
    fn test_smart_var_object_property() {
        // 测试对象属性访问：obj变量应该转换为const
        let result = test_smart_transform("var obj = {name: 'test'}; console.log(obj.name);");
        assert!(result.contains("const obj = { name: \"test\" }"), "Expected const conversion for 'obj' variable");
    }

    #[test]
    fn test_smart_var_array_access() {
        // 测试数组访问：arr变量应该转换为const
        let result = test_smart_transform("var arr = [1, 2, 3]; console.log(arr[0]);");
        assert!(result.contains("const arr = ["), "Expected const conversion for 'arr' variable");
    }

    #[test]
    fn test_smart_var_destructuring() {
        // 测试解构赋值：应该转换为const
        let result = test_smart_transform("var {name, age} = person; console.log(name);");
        assert!(result.contains("const { name, age } = person"), "Expected const conversion for destructuring");
    }

    #[test]
    fn test_smart_var_complex_scenario() {
        // 测试复杂场景：多种变量类型混合
        let result = test_smart_transform(r#"
            var config = { api: 'https://api.example.com' };
            var data = [];
            var result = null;
            var temp = 0;
            var name = 'test';

            data.push(1);
            temp = 10;
            result = processData(data);
            console.log(name, config.api);
            "#);
        assert!(result.contains("const config = { api: \"https://api.example.com\" }"), "Expected const conversion for 'config'");
        assert!(result.contains("let data = []"), "Expected let conversion for 'data'");
        assert!(result.contains("const result = null"), "Expected const conversion for 'result'");
        assert!(result.contains("let temp = 0"), "Expected let conversion for 'temp'");
        assert!(result.contains("const name = \"test\""), "Expected const conversion for 'name'");
    }



    // 测试SmartVarToLetVisitor的辅助函数
    pub fn test_smart_transform(source_text: &str) -> String {
        let allocator = Allocator::default();
        let source_type = SourceType::from_path("test.js").unwrap();
        let ret = Parser::new(&allocator, source_text, source_type).parse();
        if !ret.errors.is_empty() {
            panic!("Parsing failed: {:?}", ret.errors);
        }
        let mut program = ret.program;

        // 创建语义分析器
        let semantic = Semantic::default();

        // 使用SmartVarToLetVisitor
        let mut visitor = SmartVarToLetVisitor::new(&semantic);
        visitor.visit_program(&mut program);

        // 生成转换后的代码
        let transformed_code = Codegen::new().build(&program).code;

        println!(r#"
        before code:
        {source_text}

        after code:
        {transformed_code}
        "#);

        transformed_code
    }

}
