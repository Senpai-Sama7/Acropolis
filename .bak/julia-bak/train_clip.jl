
#!/usr/bin/env julia
# models/julia/train_clip.jl
using JSON
include("clip_contrastive.jl")
cfg = JSON.parsefile(ARGS[1])
images = rand(Float32, 224,224,3,cfg["batch_size"])
texts = rand(Int, cfg["batch_size"])
model = CLIPModel()
for epoch in 1:cfg["epochs"]
    loss = compute_loss(model, images, texts)
end
println("Multimodal training complete")
