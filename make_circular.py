from PIL import Image, ImageDraw
import os

def make_circular_transparent(input_dir, output_dir, size=48):
    """Make icons circular with transparent background"""
    os.makedirs(output_dir, exist_ok=True)
    
    icons = ["icon_idle.png", "icon_recording.png", "icon_processing.png"]
    
    for icon_name in icons:
        input_path = os.path.join(input_dir, icon_name)
        output_path = os.path.join(output_dir, icon_name)
        
        if os.path.exists(input_path):
            img = Image.open(input_path).convert("RGBA")
            
            # Resize first
            img = img.resize((size, size), Image.Resampling.LANCZOS)
            
            # Create circular mask
            mask = Image.new("L", (size, size), 0)
            draw = ImageDraw.Draw(mask)
            draw.ellipse((0, 0, size, size), fill=255)
            
            # Apply mask to create circular image with transparent corners
            output = Image.new("RGBA", (size, size), (0, 0, 0, 0))
            output.paste(img, (0, 0), mask)
            
            output.save(output_path, "PNG")
            print(f"Processed: {icon_name} -> circular {size}x{size}")
        else:
            print(f"Not found: {input_path}")

if __name__ == "__main__":
    # First reload originals from parent folder if they exist
    make_circular_transparent("assets", "assets", 48)
    print("Done!")
