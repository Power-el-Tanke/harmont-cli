module Main where

import Lib (add)
import System.Exit (exitFailure, exitSuccess)

main :: IO ()
main =
    if add 2 3 == 5 then exitSuccess else exitFailure
