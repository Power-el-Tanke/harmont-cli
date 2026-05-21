let test_add () = Alcotest.(check int) "adds" 5 (Example_lib.Lib.add 2 3)

let () =
  Alcotest.run "example"
    [ ("lib", [ Alcotest.test_case "add" `Quick test_add ]) ]
