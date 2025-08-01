
#!/usr/bin/env julia
# models/julia/train_ltn.jl
using JSON, Flux
include("ltn_logic.jl")
cfg = JSON.parsefile(ARGS[1])
data = rand(Float32, cfg["input_dim"], cfg["batch_size"])
model = LTNModel(cfg["input_dim"])
opt = ADAM(cfg["lr"])
train!(model, data, opt; epochs=cfg["epochs"])
println("Neuro-symbolic training complete")
