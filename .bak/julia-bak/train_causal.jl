
#!/usr/bin/env julia
# models/julia/train_causal.jl
using JSON, CSV, DataFrames
include("causal_model.jl")
cfg = JSON.parsefile(ARGS[1])
df = CSV.read(cfg["data_path"], DataFrame)
est = CausalEstimator(cfg["graph"])
est_value = estimate(est, df, Symbol(cfg["treatment"]), Symbol(cfg["outcome"]))
println("Estimated causal effect: ", est_value)
